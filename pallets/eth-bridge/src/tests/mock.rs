use super::*;
use crate::{self as pallet_eth_bridge};
use frame_support::{parameter_types, traits::GenesisBuild};
use frame_system as system;
use pallet_avn::testing::U64To32BytesConverter;
use sp_core::{ConstU32, ConstU64, H256};
use sp_runtime::{
    testing::{Header, TestXt, UintAuthorityId},
    traits::{BlakeTwo256, IdentityLookup},
};

pub type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<TestRuntime>;
pub type Block = frame_system::mocking::MockBlock<TestRuntime>;
pub type Extrinsic = TestXt<RuntimeCall, ()>;

use crate::{self as eth_bridge};
frame_support::construct_runtime!(
    pub enum TestRuntime where
        Block = Block,
        NodeBlock = Block,
        UncheckedExtrinsic = UncheckedExtrinsic,
    {
        System: frame_system::{Pallet, Call, Config, Storage, Event<T>},
        Timestamp: pallet_timestamp::{Pallet, Call, Storage, Inherent},
        AVN: pallet_avn::{Pallet, Storage},
        EthBridge: eth_bridge::{Pallet, Call, Storage, Event<T>},
    }
);

impl<LocalCall> frame_system::offchain::SendTransactionTypes<LocalCall> for TestRuntime
where
    RuntimeCall: From<LocalCall>,
{
    type OverarchingCall = RuntimeCall;
    type Extrinsic = Extrinsic;
}

parameter_types! {
    pub const BlockHashCount: u64 = 250;
    pub const SS58Prefix: u8 = 42;
}

impl Config for TestRuntime {
    type MaxUnresolvedTx = ConstU32<1000>;
    type RuntimeEvent = RuntimeEvent;
    type TimeProvider = pallet_timestamp::Pallet<TestRuntime>;
    type RuntimeCall = RuntimeCall;
    type WeightInfo = ();
    type AccountToBytesConvert = U64To32BytesConverter;
    type HandleEthTxResult = TestRuntime;
}

impl system::Config for TestRuntime {
    type BaseCallFilter = frame_support::traits::Everything;
    type BlockWeights = ();
    type BlockLength = ();
    type DbWeight = ();
    type RuntimeOrigin = RuntimeOrigin;
    type RuntimeCall = RuntimeCall;
    type Index = u64;
    type BlockNumber = u64;
    type Hash = H256;
    type Hashing = BlakeTwo256;
    type AccountId = u64;
    type Lookup = IdentityLookup<Self::AccountId>;
    type Header = Header;
    type RuntimeEvent = RuntimeEvent;
    type BlockHashCount = BlockHashCount;
    type Version = ();
    type PalletInfo = PalletInfo;
    type AccountData = ();
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type SystemWeightInfo = ();
    type SS58Prefix = ();
    type OnSetCode = ();
    type MaxConsumers = frame_support::traits::ConstU32<16>;
}

impl pallet_timestamp::Config for TestRuntime {
    type Moment = u64;
    type OnTimestampSet = ();
    type MinimumPeriod = ConstU64<12000>;
    type WeightInfo = ();
}

impl avn::Config for TestRuntime {
    type AuthorityId = UintAuthorityId;
    type EthereumPublicKeyChecker = ();
    type NewSessionHandler = ();
    type DisabledValidatorChecker = ();
    type FinalisedBlockChecker = ();
    type TimeProvider = pallet_timestamp::Pallet<TestRuntime>;
    type WeightInfo = ();
}

pub struct ExtBuilder {
    storage: sp_runtime::Storage,
}

impl ExtBuilder {
    pub fn build_default() -> Self {
        let storage = system::GenesisConfig::default().build_storage::<TestRuntime>().unwrap();
        Self { storage }
    }

    pub fn as_externality(self) -> sp_io::TestExternalities {
        let mut ext = sp_io::TestExternalities::from(self.storage);
        // Events do not get emitted on block 0, so we increment the block here
        ext.execute_with(|| System::set_block_number(1));
        ext
    }

    #[allow(dead_code)]
    pub fn with_genesis_config(mut self) -> Self {
        let _ = pallet_eth_bridge::GenesisConfig::<TestRuntime> {
            _phantom: Default::default(),
            eth_tx_lifetime_secs: 60 * 30,
            next_tx_id: 1,
        }
        .assimilate_storage(&mut self.storage);
        self
    }
}

impl HandleEthTxResult for TestRuntime {
    fn result(tx_id: u32, succeeded: bool) {
        println!("Tx ID: {}, Succeeded?: {}", tx_id, succeeded);
    }
}
