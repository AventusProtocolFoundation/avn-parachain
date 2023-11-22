use crate::Error;
use jsonrpsee::{
    core::{error::Error as JsonRpseeError, RpcResult as Result},
    types::error::{CallError, ErrorCode, ErrorObject},
};

pub use avn_parachain_runtime::{
    AvnProxyEvent, ChargeTransactionPayment, EthEvent, EventRecord, FrameSystem, Hash, Phase,
    SystemEvent, TokenManager, TokenManagerCall, TokenManagerEvent,
};
use codec::Encode;
use log::{debug, error};
use sc_client_api::{client::BlockBackend, UsageProvider};
use serde::{Deserialize, Serialize};
use sp_api::CallApiAt;
use sp_runtime::{
    generic::{BlockId, Era, SignedBlock},
    traits::{Block as BlockT, SaturatedConversion},
};
use sp_state_machine::InspectState;
pub use std::sync::Arc;

/// A type that represents an abi encoded leaf which can be decoded by Ethereum
pub type EncodedLeafData = Vec<u8>;

/// Filter object to uniquely identify a lower leaf
#[derive(Clone, Deserialize, Serialize, Debug)]
pub struct LowerLeafFilter {
    /// The block number that contains the lower extrinsic we want
    pub block_number: u32,
    /// The index of the extrinsic in the specified block number
    pub extrinsic_index: u32,
}

/// Gets a vector of leaves for the given block range and a filtered leaf (if found) based on the
/// filter provided
pub fn get_extrinsics_and_check_if_filter_target_exists<Block: BlockT, ClientT>(
    client: &Arc<ClientT>,
    from_block_number: u32,
    to_block_number: u32,
    filter_data: LowerLeafFilter,
) -> Result<(Option<EncodedLeafData>, Vec<EncodedLeafData>)>
where
    ClientT: BlockBackend<Block> + CallApiAt<Block> + UsageProvider<Block> + Send + Sync + 'static,
{
    let mut leaves: Vec<Vec<u8>> = vec![];
    let mut filtered_leaf: Option<EncodedLeafData> = None;

    for block_number in from_block_number..=to_block_number {
        let filter = get_filter_for_block(block_number, &filter_data);
        let (leaf, mut extrinsics) = process_extrinsics_in_block_and_check_if_filter_target_exists(
            client,
            block_number,
            filter,
        )?;
        if leaf.is_some() {
            filtered_leaf = leaf;
        }

        leaves.append(&mut extrinsics);
    }

    debug!("[RPC] filtered_leaf: {:?}", filtered_leaf);
    debug!("[RPC] leaves: {:?}", leaves);

    Ok((filtered_leaf, leaves))
}

/// Returns a tuple of a leaf it it exists and a vector of signed transactions in the given block
/// The leaf is matched against the filter data passed in.
pub fn process_extrinsics_in_block_and_check_if_filter_target_exists<Block: BlockT, ClientT>(
    client: &Arc<ClientT>,
    block_number: u32,
    filter_data: Option<&LowerLeafFilter>,
) -> Result<(Option<EncodedLeafData>, Vec<EncodedLeafData>)>
where
    ClientT: BlockBackend<Block> + CallApiAt<Block> + UsageProvider<Block> + Send + Sync + 'static,
{
    let mut filtered_leaf: Option<EncodedLeafData> = None;
    let mut leaves: Vec<Vec<u8>> = vec![];

    let signed_block: SignedBlock<Block> = get_signed_block(client, block_number)?;

    let (extrinsic_events, system_events): (Vec<_>, Vec<_>) = client
        .state_at(&BlockId::Number(block_number.into()))
        .expect("reading state_at failed")
        .inspect_state(|| {
            avn_parachain_runtime::System::events()
                .into_iter()
                .partition(|e| is_extrinsic_event(e))
        });

    for (index, tx) in signed_block.block.extrinsics().iter().enumerate() {
        let tx_execution_failed = extrinsic_events
            .iter()
            .any(|e| event_belongs_to_extrinsic(e, index) && contains_failed_event(&e));

        if !tx_execution_failed {
            leaves.push(tx.encode());
        }
    }

    // Now add any transactions triggered by OnInitialize
    // These will only have events (without a transaction) so create a wrapper
    for (index, event) in get_on_initialize_lower_events(&system_events).iter().enumerate() {
        let function = match event {
            avn_parachain_runtime::RuntimeEvent::TokenManager(TokenManagerEvent::AvtLowered {
                sender,
                recipient,
                amount,
                t1_recipient,
                schedule_nonce,
            }) => {
                let avt = TokenManager::avt_token_contract();
                TokenManagerCall::execute_lower {
                    from: sender.clone(),
                    to_account_id: recipient.clone(),
                    token_id: avt,
                    amount: *amount,
                    t1_recipient: *t1_recipient,
                    schedule_nonce: *schedule_nonce,
                }
            },
            avn_parachain_runtime::RuntimeEvent::TokenManager(
                TokenManagerEvent::TokenLowered {
                    sender,
                    recipient,
                    token_id,
                    amount,
                    t1_recipient,
                    schedule_nonce,
                },
            ) => TokenManagerCall::execute_lower {
                from: sender.clone(),
                to_account_id: recipient.clone(),
                token_id: *token_id,
                amount: *amount,
                t1_recipient: *t1_recipient,
                schedule_nonce: *schedule_nonce,
            },
            _ => continue,
        };

        let is_match = extrinsic_matches_filter(index as u32, block_number, filter_data);
        let lower_tx = create_extrinsic(function, block_number as u64, index as u32);
        let leaf = lower_tx.encode();
        error!(
            "Encoded OnInitialize lower tx. Block {:?}, Index {:?}, extrinsic: {:?}",
            block_number, index, leaf
        );
        leaves.push(leaf.clone());
        if is_match {
            filtered_leaf = Some(leaf);
        }
    }

    Ok((filtered_leaf, leaves))
}

fn event_belongs_to_extrinsic(
    event_record: &EventRecord<avn_parachain_runtime::RuntimeEvent, Hash>,
    extrinsic_index: usize,
) -> bool {
    matches!(event_record.phase, Phase::ApplyExtrinsic(i) if i == extrinsic_index as u32)
}

fn get_on_initialize_lower_events(
    system_events: &Vec<EventRecord<avn_parachain_runtime::RuntimeEvent, Hash>>,
) -> Vec<&avn_parachain_runtime::RuntimeEvent> {
    system_events
        .iter()
        .filter_map(|e| {
            if is_on_initialize_event(e) && is_lower_event(e) {
                return Some(&e.event)
            }

            None
        })
        .collect::<Vec<_>>()
}

fn is_extrinsic_event(
    event_record: &EventRecord<avn_parachain_runtime::RuntimeEvent, Hash>,
) -> bool {
    matches!(event_record.phase, Phase::ApplyExtrinsic(_))
}

fn is_on_initialize_event(
    event_record: &EventRecord<avn_parachain_runtime::RuntimeEvent, Hash>,
) -> bool {
    matches!(event_record.phase, Phase::Initialization)
}

fn is_lower_event(event_record: &EventRecord<avn_parachain_runtime::RuntimeEvent, Hash>) -> bool {
    matches!(
        event_record.event,
        avn_parachain_runtime::RuntimeEvent::TokenManager(TokenManagerEvent::AvtLowered { .. }) |
            avn_parachain_runtime::RuntimeEvent::TokenManager(
                TokenManagerEvent::TokenLowered { .. }
            )
    )
}

fn contains_failed_event(
    event_record: &EventRecord<avn_parachain_runtime::RuntimeEvent, Hash>,
) -> bool {
    matches!(
        event_record.event,
        avn_parachain_runtime::RuntimeEvent::System(SystemEvent::ExtrinsicFailed { .. }) |
            avn_parachain_runtime::RuntimeEvent::AvnProxy(AvnProxyEvent::InnerCallFailed { .. }) |
            avn_parachain_runtime::RuntimeEvent::EthereumEvents(EthEvent::EventRejected { .. })
    )
}

fn get_signed_block<Block: BlockT, ClientT>(
    client: &Arc<ClientT>,
    block_number: u32,
) -> Result<SignedBlock<Block>>
where
    ClientT: BlockBackend<Block> + CallApiAt<Block> + UsageProvider<Block> + Send + Sync + 'static,
{
    let maybe_block = client.block(&BlockId::Number(block_number.into())).map_err(|e| {
        const ERROR_MESSAGE: &str = "Error getting block data";
        error!("[RPC] {}", ERROR_MESSAGE);
        JsonRpseeError::Call(CallError::Custom(ErrorObject::owned(
            ErrorCode::ServerError(Error::ErrorGettingBlockData.into()).code(),
            ERROR_MESSAGE,
            Some(format!("{:?}", e)),
        )))
    })?;

    if maybe_block.is_none() {
        let error_message = format!("Data for block #{:?} is not found", block_number);
        error!("[RPC] {}", error_message);
        return Err(JsonRpseeError::Call(CallError::Custom(ErrorObject::owned(
            ErrorCode::ServerError(Error::BlockDataNotFound.into()).code(),
            error_message,
            None::<()>,
        ))))
    }

    let signed_block: SignedBlock<Block> = maybe_block.expect("Not empty");
    if get_latest_finalised_block(client) < block_number {
        let error_message = format!("Data for block #{:?} is not found", block_number);
        error!("[RPC] {}", error_message);
        return Err(JsonRpseeError::Call(CallError::Custom(ErrorObject::owned(
            ErrorCode::ServerError(Error::BlockNotFinalised.into()).code(),
            error_message,
            None::<()>,
        ))))
    }

    Ok(signed_block)
}

/// Returns the latest finalised block number
pub fn get_latest_finalised_block<Block: BlockT, ClientT>(client: &Arc<ClientT>) -> u32
where
    ClientT: BlockBackend<Block> + CallApiAt<Block> + UsageProvider<Block> + Send + Sync + 'static,
{
    let finalised_block_number = client.usage_info().chain.finalized_number;
    return finalised_block_number.saturated_into::<u32>()
}

fn get_filter_for_block(block_number: u32, filter: &LowerLeafFilter) -> Option<&LowerLeafFilter> {
    if filter.block_number == block_number {
        return Some(filter)
    }

    return None
}

fn extrinsic_matches_filter(
    index: u32,
    block_number: u32,
    filter: Option<&LowerLeafFilter>,
) -> bool {
    if let Some(filter) = filter {
        if filter.extrinsic_index == index && filter.block_number == block_number {
            return true
        }
    }

    return false
}

/// Create a transaction using the given function (call).
///
/// This function will only create a fake transaction object that can be be encoded/decoded
/// successfully. The data, such as Sender address, Signature and Era is all fake values
///
/// Note: If the structure of a transaction, or the signed-extra type changes in the runtime, this
/// function should also change. Use with care.
pub fn create_extrinsic(
    function: impl Into<avn_parachain_runtime::RuntimeCall>,
    block_number: u64,
    index: u32,
) -> avn_parachain_runtime::UncheckedExtrinsic {
    let function = function.into();
    let period = 0u64;
    let tip = 0;
    let extra: avn_parachain_runtime::SignedExtra = (
        FrameSystem::CheckNonZeroSender::<avn_parachain_runtime::Runtime>::new(),
        FrameSystem::CheckSpecVersion::<avn_parachain_runtime::Runtime>::new(),
        FrameSystem::CheckTxVersion::<avn_parachain_runtime::Runtime>::new(),
        FrameSystem::CheckGenesis::<avn_parachain_runtime::Runtime>::new(),
        FrameSystem::CheckEra::<avn_parachain_runtime::Runtime>::from(Era::mortal(
            period,
            block_number,
        )),
        FrameSystem::CheckNonce::<avn_parachain_runtime::Runtime>::from(index),
        FrameSystem::CheckWeight::<avn_parachain_runtime::Runtime>::new(),
        ChargeTransactionPayment::<avn_parachain_runtime::Runtime>::from(tip),
    );

    avn_parachain_runtime::UncheckedExtrinsic::new_signed(
        function,
        sp_runtime::AccountId32::new([1u8; 32]).into(),
        avn_parachain_runtime::Signature::Sr25519(sp_core::sr25519::Signature([1u8; 64])),
        extra,
    )
}
