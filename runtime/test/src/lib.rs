#![cfg_attr(not(feature = "std"), no_std)]
// `construct_runtime!` does a lot of recursion and requires us to increase the limit to 256.
#![recursion_limit = "256"]

// Make the WASM binary available.
#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

pub mod apis;
mod configs;
pub mod governance;
pub mod proxy_config;

use core::cmp::Ordering;

use codec::{Decode, Encode};
use scale_info::TypeInfo;

#[cfg(any(feature = "std", test))]
pub use sp_runtime::BuildStorage;
use sp_runtime::{create_runtime_str, generic, impl_opaque_keys};
pub use sp_runtime::{MultiAddress, Perbill, Permill, RuntimeDebug};

use sp_std::{prelude::*, vec::Vec};

#[cfg(feature = "std")]
use sp_version::NativeVersion;
use sp_version::RuntimeVersion;

use frame_support::{
    construct_runtime, parameter_types,
    traits::{
        fungible::HoldConsideration, AsEnsureOriginWithArg, ConstU32, Contains, Currency,
        LinearStoragePrice, OnUnbalanced, PrivilegeCmp,
    },
    weights::{constants::WEIGHT_REF_TIME_PER_SECOND, Weight},
};
pub use frame_system::{
    limits::{BlockLength, BlockWeights},
    EnsureRoot, EnsureSigned, Event as SystemEvent, EventRecord, Phase,
};
use governance::pallet_custom_origins;
use proxy_config::AvnProxyConfig;
pub use sp_consensus_aura::sr25519::AuthorityId as AuraId;

use pallet_im_online::sr25519::AuthorityId as ImOnlineId;
use pallet_session::historical::{self as pallet_session_historical};
use sp_authority_discovery::AuthorityId as AuthorityDiscoveryId;

use pallet_avn::sr25519::AuthorityId as AvnId;

pub use pallet_avn_proxy::{Event as AvnProxyEvent, ProvableProxy};
use pallet_eth_bridge_runtime_api::InstanceId;
use pallet_parachain_staking;
use sp_avn_common::{
    eth::EthBridgeInstance,
    event_discovery::{AdditionalEvents, EthBlockRange, EthereumEventsPartition},
    InnerCallValidator, Proof,
};

use crate::apis::RUNTIME_API_VERSIONS;
pub use node_primitives::{AccountId, Signature};
pub(crate) use node_primitives::{Balance, BlockNumber, Hash, Moment, Nonce};

use runtime_common::{
    constants::{currency::*, time::*},
    weights, Address, Header, TransactionByteFee, WeightToFee,
};

pub type NegativeImbalance<T> = <pallet_balances::Pallet<T> as Currency<
    <T as frame_system::Config>::AccountId,
>>::NegativeImbalance;

/// Block type as expected by this runtime.
pub type Block = generic::Block<Header, UncheckedExtrinsic>;

/// A Block signed with a Justification
pub type SignedBlock = generic::SignedBlock<Block>;

/// BlockId type as expected by this runtime.
pub type BlockId = generic::BlockId<Block>;

/// The SignedExtension to the basic transaction logic.
#[docify::export(template_signed_extra)]
pub type SignedExtra = (
    frame_system::CheckNonZeroSender<Runtime>,
    frame_system::CheckSpecVersion<Runtime>,
    frame_system::CheckTxVersion<Runtime>,
    frame_system::CheckGenesis<Runtime>,
    frame_system::CheckEra<Runtime>,
    frame_system::CheckNonce<Runtime>,
    frame_system::CheckWeight<Runtime>,
    pallet_transaction_payment::ChargeTransactionPayment<Runtime>,
    cumulus_primitives_storage_weight_reclaim::StorageWeightReclaim<Runtime>,
    frame_metadata_hash_extension::CheckMetadataHash<Runtime>,
);

/// Unchecked extrinsic type as expected by this runtime.
pub type UncheckedExtrinsic =
    generic::UncheckedExtrinsic<Address, RuntimeCall, Signature, SignedExtra>;

/// Executive: handles dispatch to the various modules.
pub type Executive = frame_executive::Executive<
    Runtime,
    Block,
    frame_system::ChainContext<Runtime>,
    Runtime,
    AllPalletsWithSystem,
    (pallet_validators_manager::migration::ValidatorsManagerMigrations<Runtime>,),
>;

impl_opaque_keys! {
    pub struct SessionKeys {
        pub aura: Aura,
        pub authority_discovery: AuthorityDiscovery,
        pub im_online: ImOnline,
        pub avn: Avn,
    }
}

#[sp_version::runtime_version]
pub const VERSION: RuntimeVersion = RuntimeVersion {
    spec_name: create_runtime_str!("avn-test-parachain"),
    impl_name: create_runtime_str!("avn-test-parachain"),
    authoring_version: 1,
    spec_version: 134,
    impl_version: 0,
    apis: RUNTIME_API_VERSIONS,
    transaction_version: 1,
    state_version: 1,
};

/// This determines the average expected block time that we are targeting.
/// Blocks will be produced at a minimum duration defined by `SLOT_DURATION`.
/// `SLOT_DURATION` is picked up by `pallet_timestamp` which is in turn picked
/// up by `pallet_aura` to implement `fn slot_duration()`.

// NOTE: Currently it is not possible to change the slot duration after the chain has started.
//       Attempting to do so will brick block production.
pub const SLOT_DURATION: u64 = MILLISECS_PER_BLOCK;

/// The existential deposit. Set to 1/10 of the Connected Relay Chain.
pub const EXISTENTIAL_DEPOSIT: Balance = 0;

/// We assume that ~5% of the block weight is consumed by `on_initialize` handlers. This is
/// used to limit the maximal weight of a single extrinsic.
const AVERAGE_ON_INITIALIZE_RATIO: Perbill = Perbill::from_percent(5);

/// We allow `Normal` extrinsics to fill up the block up to 75%, the rest can be used by
/// `Operational` extrinsics.
const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);

/// We allow for 2 seconds of compute with a 6 second average block time.
const MAXIMUM_BLOCK_WEIGHT: Weight = Weight::from_parts(
    WEIGHT_REF_TIME_PER_SECOND.saturating_mul(2),
    cumulus_primitives_core::relay_chain::MAX_POV_SIZE as u64,
);

/// Maximum number of blocks simultaneously accepted by the Runtime, not yet included
/// into the relay chain.
const UNINCLUDED_SEGMENT_CAPACITY: u32 = 3;
/// How many parachain blocks are processed by the relay chain per parent. Limits the
/// number of blocks authored per slot.
const BLOCK_PROCESSING_VELOCITY: u32 = 1;
/// Relay chain slot duration, in milliseconds.
const RELAY_CHAIN_SLOT_DURATION_MILLIS: u32 = 6000;

/// Aura consensus hook
type ConsensusHook = cumulus_pallet_aura_ext::FixedVelocityConsensusHook<
    Runtime,
    RELAY_CHAIN_SLOT_DURATION_MILLIS,
    BLOCK_PROCESSING_VELOCITY,
    UNINCLUDED_SEGMENT_CAPACITY,
>;

/// The version information used to identify this runtime when compiled natively.
#[cfg(feature = "std")]
pub fn native_version() -> NativeVersion {
    NativeVersion { runtime_version: VERSION, can_author_with: Default::default() }
}

/// Use this filter to block users from calling extrinsics listed here.
pub struct RestrictedEndpointFilter;
impl Contains<RuntimeCall> for RestrictedEndpointFilter {
    fn contains(c: &RuntimeCall) -> bool {
        !matches!(
            c,
            RuntimeCall::ParachainStaking(pallet_parachain_staking::Call::join_candidates { .. }) |
                RuntimeCall::ParachainStaking(
                    pallet_parachain_staking::Call::schedule_leave_candidates { .. }
                ) |
                RuntimeCall::ParachainStaking(
                    pallet_parachain_staking::Call::execute_leave_candidates { .. }
                ) |
                RuntimeCall::ParachainStaking(
                    pallet_parachain_staking::Call::cancel_leave_candidates { .. }
                ) /* Allow the following direct staking extrinsics: */
                  /*
                      Call::ParachainStaking(pallet_parachain_staking::Call::nominate {..}) |
                      Call::ParachainStaking(pallet_parachain_staking::Call::nominator_bond_more {..}) |
                      Call::ParachainStaking(pallet_parachain_staking::Call::schedule_nominator_bond_less {..}) |
                      Call::ParachainStaking(pallet_parachain_staking::Call::schedule_revoke_nomination {..}) |
                      Call::ParachainStaking(pallet_parachain_staking::Call::execute_nomination_request {..}) |
                      Call::ParachainStaking(pallet_parachain_staking::Call::cancel_nomination_request {..}) |

                      Call::ParachainStaking(pallet_parachain_staking::Call::schedule_candidate_bond_less {..}) |
                      Call::ParachainStaking(pallet_parachain_staking::Call::execute_candidate_bond_less {..}) |
                      Call::ParachainStaking(pallet_parachain_staking::Call::cancel_candidate_bond_less {..}) |

                      Call::ParachainStaking(pallet_parachain_staking::Call::schedule_leave_nominators {..}) |
                      Call::ParachainStaking(pallet_parachain_staking::Call::execute_leave_nominators {..}) |
                      Call::ParachainStaking(pallet_parachain_staking::Call::cancel_leave_nominators {..}) |

                      Call::ParachainStaking(pallet_parachain_staking::Call::hotfix_remove_nomination_requests_exited_candidates{..})
                  */
        )
    }
}

pub type MainEthBridge = pallet_eth_bridge::Instance1;
pub type SecondaryEthBridge = pallet_eth_bridge::Instance2;
const MAIN_ETH_BRIDGE_ID: u8 = 1u8;
const SECONDARY_ETH_BRIDGE_ID: u8 = 2u8;

// Create the runtime by composing the FRAME pallets that were previously configured.
construct_runtime!(
    pub enum Runtime
    {
        // System support stuff.
        System: frame_system = 0,
        ParachainSystem: cumulus_pallet_parachain_system = 1,
        Timestamp: pallet_timestamp = 2,
        ParachainInfo: parachain_info = 3,

        // Monetary stuff.
        Balances: pallet_balances::{Pallet, Call, Storage, Config<T>, Event<T>} = 10,
        TransactionPayment: pallet_transaction_payment::{Pallet, Storage, Event<T>} = 11,

        // Collator support. The order of these 4 are important and shall not change.
        Authorship: pallet_authorship = 20,
        Session: pallet_session = 22,
        Aura: pallet_aura = 23,
        AuraExt: cumulus_pallet_aura_ext = 24,
        ParachainStaking: pallet_parachain_staking = 96,

        // Since the ValidatorsManager integrates with the ParachainStaking pallet, we want to initialise after it.
        ValidatorsManager: pallet_validators_manager = 18,

        // XCM helpers.
        XcmpQueue: cumulus_pallet_xcmp_queue = 30,
        PolkadotXcm: pallet_xcm = 31,
        CumulusXcm: cumulus_pallet_xcm = 32,
        MessageQueue: pallet_message_queue = 33,

        // Substrate pallets
        Assets: pallet_assets = 60,
        Sudo: pallet_sudo = 62,
        AuthorityDiscovery: pallet_authority_discovery = 70,
        Historical: pallet_session_historical::{Pallet} = 71,
        Offences: pallet_offences = 72,
        ImOnline: pallet_im_online = 73,
        Utility: pallet_utility = 74,

        // Rest of AvN pallets
        Avn: pallet_avn = 81,
        AvnOffenceHandler: pallet_avn_offence_handler = 83,
        EthereumEvents: pallet_ethereum_events = 84,
        NftManager: pallet_nft_manager = 86,
        TokenManager: pallet_token_manager = 87,
        Summary: pallet_summary::<Instance1> = 88,
        AvnProxy: pallet_avn_proxy = 89,
        EthBridge: pallet_eth_bridge::<Instance1> = 91,
        AvnAnchor: pallet_avn_anchor = 92,
        AnchorSummary: pallet_summary::<Instance2> = 110,
        EthSecondBridge: pallet_eth_bridge::<Instance2> = 111,

         // OpenGov pallets
        Preimage: pallet_preimage::{Pallet, Call, Storage, Event<T>, HoldReason} = 97,
        Scheduler: pallet_scheduler::{Pallet, Storage, Event<T>, Call} = 98,
        Origins: pallet_custom_origins::{Origin} = 99,
        ConvictionVoting: pallet_conviction_voting::{Pallet, Call, Storage, Event<T>} = 100,
        Referenda: pallet_referenda::{Pallet, Call, Storage, Event<T>} = 101,
        Whitelist: pallet_whitelist::{Pallet, Call, Storage, Event<T>} = 102
    }
);

#[cfg(feature = "runtime-benchmarks")]
mod benchmarks;

cumulus_pallet_parachain_system::register_validate_block! {
    Runtime = Runtime,
    BlockExecutor = cumulus_pallet_aura_ext::BlockExecutor::<Runtime, Executive>,
}
