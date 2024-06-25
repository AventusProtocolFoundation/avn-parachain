use crate as pallet_your_pallet;
use frame_support::traits::{ConstU16, ConstU32, ConstU64};
use sp_core::H256;
use sp_runtime::{
    testing::Header,
    traits::{BlakeTwo256, IdentityLookup},
};

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
    pub enum Test where
        Block = Block,
        NodeBlock = Block,
        UncheckedExtrinsic = UncheckedExtrinsic,
    {
        System: frame_system,
        YourPallet: pallet_your_pallet,
        ConvictionVoting: pallet_conviction_voting,
    }
);

impl frame_system::Config for Test {
    type BaseCallFilter = Everything;
    type DbWeight = ();
    type RuntimeOrigin = RuntimeOrigin;
    type Nonce = u64;
    type RuntimeCall = RuntimeCall;
    type Hash = H256;
    type Hashing = BlakeTwo256;
    type AccountId = AccountId;
    type Lookup = IdentityLookup<Self::AccountId>;
    type Block = Block;
    type RuntimeEvent = RuntimeEvent;
    type BlockHashCount = BlockHashCount;
    type Version = ();
    type PalletInfo = PalletInfo;
    type AccountData = pallet_balances::AccountData<Balance>;
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type SystemWeightInfo = ();
    type BlockWeights = RuntimeBlockWeights;
    type BlockLength = BlockLength;
    type SS58Prefix = SS58Prefix;
    type OnSetCode = ();
    type MaxConsumers = frame_support::traits::ConstU32<16>;
}

impl pallet_conviction_voting::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type Currency = ();
    type VoteLockingPeriod = ConstU64<100>;
    type MaxVotes = ConstU32<100>;
    type WeightInfo = ();
    type MaxTurnout = ();
    type Polls = ();
}

impl pallet_your_pallet::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = ();
    type TimeProvider = ();
    type MaxVoteAge = ConstU64<100>;
    type Moment = u64;
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    let t = frame_system::GenesisConfig::default().build_storage::<Test>().unwrap();
    sp_io::TestExternalities::new(t)
}
