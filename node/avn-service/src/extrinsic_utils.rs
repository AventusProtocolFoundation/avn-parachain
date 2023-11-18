use crate::Error;
use jsonrpsee::{
    core::{error::Error as JsonRpseeError, RpcResult as Result},
    types::error::{CallError, ErrorCode, ErrorObject},
};

use codec::Encode;
use log::{debug, error};
use sc_client_api::{client::BlockBackend, UsageProvider};
use sp_api::CallApiAt;
use sp_state_machine::InspectState;
use serde::{Deserialize, Serialize};
use sp_runtime::{
    generic::{BlockId, SignedBlock},
    traits::{Block as BlockT, SaturatedConversion},
};
pub use std::sync::Arc;
pub use avn_parachain_runtime::{Phase, EventRecord, SystemEvent, AvnProxyEvent, EthEvent, Hash};

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
    ClientT: BlockBackend<Block> + CallApiAt<Block> +  UsageProvider<Block> + Send + Sync + 'static,
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
    ClientT: BlockBackend<Block> + CallApiAt<Block> +  UsageProvider<Block> + Send + Sync + 'static,
{
    let mut filtered_leaf: Option<EncodedLeafData> = None;
    let mut leaves: Vec<Vec<u8>> = vec![];

    let signed_block: SignedBlock<Block> = get_signed_block(client, block_number)?;

    let block_events = client.state_at(&BlockId::Number(block_number.into())).expect("reading state_at failed").inspect_state(|| {
        avn_parachain_runtime::System::events()
    });

    for (index, tx) in signed_block.block.extrinsics().iter().enumerate() {
        let is_match = extrinsic_matches_filter(index as u32, block_number, filter_data);
        let tx_execution_failed = block_events
            .iter()
            .any(|e| event_belongs_to_extrinsic(e, index) && contains_failed_event(&e));

        if !tx_execution_failed {
            let leaf = tx.encode();

            leaves.push(leaf.clone());
            if is_match {
                filtered_leaf = Some(leaf.clone());
            }
        }
    }

    Ok((filtered_leaf, leaves))
}

fn event_belongs_to_extrinsic(event_record: &EventRecord<avn_parachain_runtime::RuntimeEvent, Hash>, extrinsic_index: usize) -> bool {
    if let Phase::ApplyExtrinsic(i) = event_record.phase {
        i == extrinsic_index as u32
    } else {
        false
    }
}

fn contains_failed_event(event_record: &EventRecord<avn_parachain_runtime::RuntimeEvent, Hash>) -> bool {
    matches!(
        event_record.event,
        avn_parachain_runtime::RuntimeEvent::System(SystemEvent::ExtrinsicFailed{ .. })
            | avn_parachain_runtime::RuntimeEvent::AvnProxy(AvnProxyEvent::InnerCallFailed { .. })
            | avn_parachain_runtime::RuntimeEvent::EthereumEvents(EthEvent::EventRejected { .. })
    )
}

fn get_signed_block<Block: BlockT, ClientT>(
    client: &Arc<ClientT>,
    block_number: u32,
) -> Result<SignedBlock<Block>>
where
    ClientT: BlockBackend<Block> + CallApiAt<Block> +  UsageProvider<Block> + Send + Sync + 'static,
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
