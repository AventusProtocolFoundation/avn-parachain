use crate::{self as avn_anchor, *};
use codec::{Decode, Encode};
use frame_support::{
    pallet_prelude::*,
    parameter_types,
    traits::{ConstU16, ConstU32, ConstU64, Everything},
};
use frame_system as system;
use pallet_avn_proxy::{self as avn_proxy, ProvableProxy};
use scale_info::TypeInfo;
use sp_avn_common::{InnerCallValidator, Proof};
use sp_core::{sr25519, H256};
use sp_keystore::{testing::MemoryKeystore, KeystoreExt};
use sp_runtime::{
    traits::{BlakeTwo256, IdentityLookup, Verify},
    BuildStorage,
};
use std::sync::Arc;

type Block = frame_system::mocking::MockBlock<TestRuntime>;

pub type Signature = sr25519::Signature;
pub type Balance = u128;
pub type AccountId = <Signature as Verify>::Signer;

frame_support::construct_runtime!(
    pub enum TestRuntime
    {
        System: frame_system::{Pallet, Call, Config<T>, Storage, Event<T>},
        Balances: pallet_balances::{Pallet, Call, Storage, Config<T>, Event<T>},
        Avn: pallet_avn::{Pallet, Storage, Event},
        AvnProxy: avn_proxy::{Pallet, Call, Storage, Event<T>},
        AvnAnchor: avn_anchor::{Pallet, Call, Storage, Event<T>},
    }
);

impl system::Config for TestRuntime {
    type BaseCallFilter = Everything;
    type BlockWeights = ();
    type BlockLength = ();
    type DbWeight = ();
    type RuntimeOrigin = RuntimeOrigin;
    type RuntimeCall = RuntimeCall;
    type Nonce = u64;
    type Hash = H256;
    type Hashing = BlakeTwo256;
    type AccountId = AccountId;
    type Lookup = IdentityLookup<Self::AccountId>;
    type Block = Block;
    type RuntimeEvent = RuntimeEvent;
    type BlockHashCount = ConstU64<250>;
    type Version = ();
    type PalletInfo = PalletInfo;
    type AccountData = pallet_balances::AccountData<Balance>;
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type SystemWeightInfo = ();
    type SS58Prefix = ConstU16<42>;
    type OnSetCode = ();
    type MaxConsumers = ConstU32<16>;
}

parameter_types! {
    pub const ExistentialDeposit: Balance = 1;
}

impl pallet_balances::Config for TestRuntime {
    type MaxLocks = ConstU32<50>;
    type Balance = Balance;
    type RuntimeEvent = RuntimeEvent;
    type DustRemoval = ();
    type ExistentialDeposit = ExistentialDeposit;
    type AccountStore = System;
    type WeightInfo = ();
    type MaxReserves = ();
    type ReserveIdentifier = [u8; 8];
    type FreezeIdentifier = ();
    type MaxHolds = ();
    type RuntimeHoldReason = ();
    type MaxFreezes = ();
}

impl pallet_avn::Config for TestRuntime {
    type RuntimeEvent = RuntimeEvent;
    type AuthorityId = sp_runtime::testing::UintAuthorityId;
    type EthereumPublicKeyChecker = ();
    type NewSessionHandler = ();
    type DisabledValidatorChecker = ();
    type WeightInfo = ();
}

impl avn_proxy::Config for TestRuntime {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeCall = RuntimeCall;
    type Currency = Balances;
    type Public = AccountId;
    type Signature = Signature;
    type ProxyConfig = TestAvnProxyConfig;
    type WeightInfo = ();
}

impl Config for TestRuntime {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeCall = RuntimeCall;
    type Public = AccountId;
    type Signature = Signature;
    type WeightInfo = default_weights::SubstrateWeight<TestRuntime>;
}
// Test Avn proxy configuration logic
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Encode, Decode, Debug, TypeInfo)]
pub struct TestAvnProxyConfig {}
impl Default for TestAvnProxyConfig {
    fn default() -> Self {
        TestAvnProxyConfig {}
    }
}

impl ProvableProxy<RuntimeCall, Signature, AccountId> for TestAvnProxyConfig {
    fn get_proof(call: &RuntimeCall) -> Option<Proof<Signature, AccountId>> {
        match call {
            RuntimeCall::AvnAnchor(avn_anchor::Call::signed_register_chain_handler {
                proof,
                ..
            })
            | RuntimeCall::AvnAnchor(avn_anchor::Call::signed_update_chain_handler {
                proof, ..
            })
            | RuntimeCall::AvnAnchor(avn_anchor::Call::signed_submit_checkpoint_with_identity {
                proof,
                ..
            }) => Some(proof.clone()),
            _ => None,
        }
    }
}

impl InnerCallValidator for TestAvnProxyConfig {
    type Call = RuntimeCall;

    fn signature_is_valid(call: &Box<Self::Call>) -> bool {
        match **call {
            RuntimeCall::System(..) => return true,
            RuntimeCall::AvnAnchor(..) => return AvnAnchor::signature_is_valid(call),
            _ => false,
        }
    }
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    let keystore = MemoryKeystore::new();
    let t = system::GenesisConfig::<TestRuntime>::default().build_storage().unwrap();
    let mut ext = sp_io::TestExternalities::new(t);
    ext.register_extension(KeystoreExt(Arc::new(keystore)));
    ext.execute_with(|| System::set_block_number(1));
    ext
}

pub fn proxy_event_emitted(
    relayer: AccountId,
    call_hash: <TestRuntime as system::Config>::Hash,
) -> bool {
    System::events().iter().any(|a| {
        a.event
            == RuntimeEvent::AvnProxy(avn_proxy::Event::<TestRuntime>::CallDispatched {
                relayer,
                hash: call_hash,
            })
    })
}

pub fn inner_call_failed_event_emitted(call_dispatch_error: DispatchError) -> bool {
    System::events().iter().any(|a| match a.event {
        RuntimeEvent::AvnProxy(avn_proxy::Event::<TestRuntime>::InnerCallFailed {
            dispatch_error,
            ..
        }) => dispatch_error == call_dispatch_error,
        _ => false,
    })
}