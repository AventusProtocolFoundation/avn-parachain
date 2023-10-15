//Copyright 2022 Aventus Systems (UK) Ltd.

use crate::{self as avn_offence_handler, *};
use frame_support::{
    dispatch::{DispatchError, DispatchResult},
    parameter_types,
    traits::GenesisBuild,
    BasicExternalities,
};
use frame_system as system;
use pallet_session as session;
use sp_core::H256;
use sp_runtime::{
    testing::{Header, UintAuthorityId},
    traits::{BlakeTwo256, ConvertInto, IdentityLookup},
};

use std::cell::RefCell;

pub const VALIDATOR_ID_1: u64 = 1;
pub const VALIDATOR_ID_2: u64 = 2;
pub const VALIDATOR_ID_CAN_CAUSE_SLASH_ERROR: u64 = 3;

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<TestRuntime>;
type Block = frame_system::mocking::MockBlock<TestRuntime>;

frame_support::construct_runtime!(
    pub enum TestRuntime where
        Block = Block,
        NodeBlock = Block,
        UncheckedExtrinsic = UncheckedExtrinsic,
    {
        System: frame_system::{Pallet, Call, Config, Storage, Event<T>},
        Session: pallet_session::{Pallet, Call, Storage, Event, Config<T>},
        AVN: pallet_avn::{Pallet, Storage, Event},
        AvnOffenceHandler: avn_offence_handler::{Pallet, Call, Storage, Event<T>},
    }
);

pub type ValidatorId = <TestRuntime as session::Config>::ValidatorId;

impl Config for TestRuntime {
    type RuntimeEvent = RuntimeEvent;
    type Enforcer = Self;
    type WeightInfo = ();
}

impl pallet_session::historical::Config for TestRuntime {
    type FullIdentification = u64;
    type FullIdentificationOf = ConvertInto;
}

impl pallet_avn::Config for TestRuntime {
    type RuntimeEvent = RuntimeEvent;
    type AuthorityId = UintAuthorityId;
    type EthereumPublicKeyChecker = ();
    type NewSessionHandler = ();
    type DisabledValidatorChecker = ();
    type WeightInfo = ();
}

pub struct TestSessionManager;

impl session::SessionManager<u64> for TestSessionManager {
    fn new_session(_new_index: SessionIndex) -> Option<Vec<u64>> {
        None
    }
    fn end_session(_: SessionIndex) {}
    fn start_session(_: SessionIndex) {}
}

parameter_types! {
    pub const Period: u64 = 1;
    pub const Offset: u64 = 0;
}

impl session::Config for TestRuntime {
    type SessionManager = TestSessionManager;
    type Keys = UintAuthorityId;
    type ShouldEndSession = session::PeriodicSessions<Period, Offset>;
    type SessionHandler = (AVN,);
    type RuntimeEvent = RuntimeEvent;
    type ValidatorId = u64;
    type ValidatorIdOf = ConvertInto;
    type NextSessionRotation = session::PeriodicSessions<Period, Offset>;
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

impl Enforcer<ValidatorId> for TestRuntime {
    fn slash_validator(slashed_validator_id: &ValidatorId) -> DispatchResult {
        if slashed_validator_id == &VALIDATOR_ID_CAN_CAUSE_SLASH_ERROR {
            return Err(DispatchError::Other("Slash validator failed"))
        }
        Ok(())
    }
}

thread_local! {
    static VALIDATORS: RefCell<Option<Vec<u64>>> = RefCell::new(Some(vec![
        VALIDATOR_ID_1,
        VALIDATOR_ID_2,
        VALIDATOR_ID_CAN_CAUSE_SLASH_ERROR,
    ]));
}

pub struct ExtBuilder {
    pub storage: sp_runtime::Storage,
}

impl ExtBuilder {
    pub fn build_default() -> Self {
        let storage =
            frame_system::GenesisConfig::default().build_storage::<TestRuntime>().unwrap();
        Self { storage }
    }

    pub fn as_externality(self) -> sp_io::TestExternalities {
        let mut ext = sp_io::TestExternalities::from(self.storage);
        // Events do not get emitted on block 0, so we increment the block here
        ext.execute_with(|| frame_system::Pallet::<TestRuntime>::set_block_number(1u32.into()));
        ext
    }

    pub fn with_validators(mut self) -> Self {
        let validators: Vec<u64> = VALIDATORS.with(|l| l.borrow_mut().take().unwrap());
        BasicExternalities::execute_with_storage(&mut self.storage, || {
            for ref k in &validators {
                frame_system::Pallet::<TestRuntime>::inc_providers(k);
            }
        });
        let _ = session::GenesisConfig::<TestRuntime> {
            keys: validators.into_iter().map(|v| (v, v, UintAuthorityId(v))).collect(),
        }
        .assimilate_storage(&mut self.storage);
        self
    }
}

impl AvnOffenceHandler {
    pub fn enable_offence() {
        <SlashingEnabled<TestRuntime>>::put(true);
    }

    pub fn disable_offence() {
        <SlashingEnabled<TestRuntime>>::put(false);
    }
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
