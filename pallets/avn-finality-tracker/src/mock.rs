// Copyright 2022 Aventus Network Services (UK) Ltd.

#![cfg(test)]

use crate::{self as avn_finality_tracker,*};
use frame_support::parameter_types;
use frame_system as system;
use sp_core::H256;
use sp_runtime::{
    testing::{Header, TestXt, UintAuthorityId},
    traits::{BlakeTwo256, IdentityLookup},
};

pub type Extrinsic = TestXt<RuntimeCall, ()>;

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<TestRuntime>;
type Block = frame_system::mocking::MockBlock<TestRuntime>;

frame_support::construct_runtime!(
    pub enum TestRuntime where
        Block = Block,
        NodeBlock = Block,
        UncheckedExtrinsic = UncheckedExtrinsic,
    {
        System: frame_system::{Pallet, Call, Config, Storage, Event<T>},
        AVN: pallet_avn::{Pallet, Storage},
        AvnFinalityTracker: avn_finality_tracker::{Pallet, Config, Call, Storage, Event<T>},
    }
);

parameter_types! {
    pub const CacheAge: u64 = 10;
    pub const SubmissionInterval: u64 = 5;
    pub const ReportLatency: u64 = 1000;
}

impl Config for TestRuntime {
    type Event = RuntimeEvent;
    type CacheAge = CacheAge;
    type SubmissionInterval = SubmissionInterval;
    type ReportLatency = ReportLatency;
    type WeightInfo = ();
}

parameter_types! {
    pub const BlockHashCount: u64 = 250;
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

impl avn::Config for TestRuntime {
    type AuthorityId = UintAuthorityId;
    type EthereumPublicKeyChecker = ();
    type NewSessionHandler = ();
    type DisabledValidatorChecker = ();
    type FinalisedBlockChecker = ();
}

impl<LocalCall> frame_system::offchain::SendTransactionTypes<LocalCall> for TestRuntime
where
RuntimeCall: From<LocalCall>,
{
    type OverarchingCall = RuntimeCall;
    type Extrinsic = Extrinsic;
}

pub struct TestExternalitiesBuilder {
    _existential_deposit: u64,
}

impl Default for TestExternalitiesBuilder {
    fn default() -> Self {
        Self { _existential_deposit: 1 }
    }
}

impl TestExternalitiesBuilder {
    // Build a genesis storage key/value store
    pub fn build<R>(self, execute: impl FnOnce() -> R) -> sp_io::TestExternalities {
        let storage =
            frame_system::GenesisConfig::default().build_storage::<TestRuntime>().unwrap();

        let mut ext = sp_io::TestExternalities::new(storage);
        ext.execute_with(|| {
            System::set_block_number(1);
        });
        ext.execute_with(execute);
        ext
    }
}
