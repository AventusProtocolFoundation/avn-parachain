// Copyright 2020 Artos Systems (UK) Ltd.

#![cfg(test)]

use crate::{self as pallet_avn, *};
use frame_support::{
    parameter_types,
    traits::ValidatorRegistration, //, GenesisBuild},
    weights::Weight,
    BasicExternalities,
    PalletId,
};
use frame_system::{self as system, EnsureRoot};
use pallet_balances;
use pallet_collator_selection::IdentityCollator;
use pallet_session as session;
use sp_core::{
    offchain::testing::{OffchainState, PendingRequest},
    H256,
};
use sp_runtime::{
    testing::{Header, UintAuthorityId},
    traits::{BlakeTwo256, ConvertInto, IdentityLookup},
    Perbill,
};
use std::cell::RefCell;

// This is added to allow configuring pallet_session GenesisConfig
use crate::mock::sp_api_hidden_includes_construct_runtime::hidden_include::traits::GenesisBuild;

pub mod extension_builder;
use crate::mock::extension_builder::ExtBuilder;

pub type AccountId = <TestRuntime as system::Config>::AccountId;
pub type AVN = Pallet<TestRuntime>;

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<TestRuntime>;
type Block = frame_system::mocking::MockBlock<TestRuntime>;

frame_support::construct_runtime!(
    pub enum TestRuntime where
        Block = Block,
        NodeBlock = Block,
        UncheckedExtrinsic = UncheckedExtrinsic,
    {
        System: frame_system::{Pallet, Call, Config, Storage, Event<T>},
        CollatorSelection: pallet_collator_selection::{Pallet, Call, Event<T>, Storage},
        Session: pallet_session::{Pallet, Call, Storage, Event, Config<T>},
        Avn: pallet_avn::{Pallet, Storage},
        Balances: pallet_balances::{Pallet, Call, Storage, Event<T>},
    }
);

impl Config for TestRuntime {
    type AuthorityId = UintAuthorityId;
    type EthereumPublicKeyChecker = ();
    type NewSessionHandler = ();
    type DisabledValidatorChecker = ();
    type FinalisedBlockChecker = ();
}

parameter_types! {
    pub const BlockHashCount: u64 = 250;
    pub const MaximumBlockWeight: Weight = 1024;
    pub const MaximumBlockLength: u32 = 2 * 1024;
    pub const AvailableBlockRatio: Perbill = Perbill::from_percent(75);
    pub const ChallengePeriod: u64 = 2;
}

impl system::Config for TestRuntime {
    type BaseCallFilter = frame_support::traits::Everything;
    type BlockWeights = ();
    type BlockLength = ();
    type DbWeight = ();
    type Origin = Origin;
    type Index = u64;
    type BlockNumber = u64;
    type Call = Call;
    type Hash = H256;
    type Hashing = BlakeTwo256;
    type AccountId = u64;
    type Lookup = IdentityLookup<Self::AccountId>;
    type Header = Header;
    type Event = Event;
    type BlockHashCount = BlockHashCount;
    type Version = ();
    type PalletInfo = PalletInfo;
    type AccountData = pallet_balances::AccountData<u128>;
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type SystemWeightInfo = ();
    type SS58Prefix = ();
    type OnSetCode = ();
    type MaxConsumers = frame_support::traits::ConstU32<16>;
}

parameter_types! {
    pub const Period: u64 = 1;
    pub const Offset: u64 = 0;
    pub const DisabledValidatorsThreshold: Perbill = Perbill::from_percent(33);
}

pub struct IsRegistered;
impl ValidatorRegistration<AccountId> for IsRegistered {
    fn is_registered(_id: &AccountId) -> bool {
        true
    }
}

parameter_types! {
    pub const PotId: PalletId = PalletId(*b"PotStake");
    pub const MaxCandidates: u32 = 20;
    pub const MaxInvulnerables: u32 = 20;
    pub const MinCandidates: u32 = 0;
    pub const KickThreshold: u64 = 1000000;
}

impl pallet_collator_selection::Config for TestRuntime {
    type Event = Event;
    type Currency = Balances;
    type UpdateOrigin = EnsureRoot<AccountId>;
    type PotId = PotId;
    type MaxCandidates = MaxCandidates;
    type MinCandidates = MinCandidates;
    type MaxInvulnerables = MaxInvulnerables;
    type ValidatorId = <Self as frame_system::Config>::AccountId;
    type ValidatorIdOf = IdentityCollator;
    type ValidatorRegistration = IsRegistered;
    type KickThreshold = KickThreshold;
    type WeightInfo = ();
}

parameter_types! {
    pub const ExistentialDeposit: u64 = 0u64;
}

impl pallet_balances::Config for TestRuntime {
    type MaxLocks = frame_support::traits::ConstU32<1024>;
    type MaxReserves = ();
    type ReserveIdentifier = [u8; 8];
    type Balance = u128;
    type Event = Event;
    type DustRemoval = ();
    type ExistentialDeposit = ExistentialDeposit;
    type AccountStore = System;
    type WeightInfo = ();
}

thread_local! {
    // validator accounts (aka public addresses, public keys-ish)
    pub static VALIDATORS: RefCell<Option<Vec<u64>>> = RefCell::new(Some(vec![1, 2, 3]));
}

pub type SessionIndex = u32;

pub struct TestSessionManager;
impl session::SessionManager<u64> for TestSessionManager {
    fn new_session(_new_index: SessionIndex) -> Option<Vec<u64>> {
        VALIDATORS.with(|l| l.borrow_mut().take())
    }
    fn end_session(_: SessionIndex) {}
    fn start_session(_: SessionIndex) {}
}

impl session::Config for TestRuntime {
    type SessionManager = TestSessionManager;
    type Keys = UintAuthorityId;
    type ShouldEndSession = session::PeriodicSessions<Period, Offset>;
    type SessionHandler = (AVN,);
    type Event = Event;
    type ValidatorId = u64;
    type ValidatorIdOf = ConvertInto;
    type NextSessionRotation = session::PeriodicSessions<Period, Offset>;
    type WeightInfo = ();
}

impl ExtBuilder {
    pub fn with_validators(mut self) -> Self {
        let validators: Vec<u64> = VALIDATORS.with(|l| l.borrow_mut().take().unwrap());

        BasicExternalities::execute_with_storage(&mut self.storage, || {
            for ref k in &validators {
                frame_system::Pallet::<TestRuntime>::inc_providers(k);
            }
        });

        let _ = pallet_session::GenesisConfig::<TestRuntime> {
            keys: validators.into_iter().map(|v| (v, v, UintAuthorityId(v))).collect(),
        }
        .assimilate_storage(&mut self.storage);
        self
    }
}

/************* Test helpers ************ */

#[allow(dead_code)]
pub fn keys_setup_return_good_validator(
) -> Validator<<TestRuntime as Config>::AuthorityId, AccountId> {
    let validators = AVN::validators(); // Validators are tuples (UintAuthorityId(int), int)
    assert_eq!(validators[0], Validator { account_id: 1, key: UintAuthorityId(1) });
    assert_eq!(validators[2], Validator { account_id: 3, key: UintAuthorityId(3) });
    assert_eq!(validators.len(), 3);

    // AuthorityId type for TestRuntime is UintAuthorityId
    let keys: Vec<UintAuthorityId> = validators.into_iter().map(|v| v.key).collect();
    UintAuthorityId::set_all_keys(keys); // Keys in the setup are either () or (1,2,3). See VALIDATORS.
    let current_node_validator = AVN::get_validator_for_current_node().unwrap(); // filters validators() to just those corresponding to this validator
    assert_eq!(current_node_validator.key, UintAuthorityId(1));
    assert_eq!(current_node_validator.account_id, 1);

    assert_eq!(current_node_validator, Validator { account_id: 1, key: UintAuthorityId(1) });

    return current_node_validator
}

#[allow(dead_code)]
pub fn bad_authority() -> Validator<<TestRuntime as Config>::AuthorityId, AccountId> {
    let validator = Validator { account_id: 0, key: UintAuthorityId(0) };

    return validator
}

#[allow(dead_code)]
pub fn mock_get_request(state: &mut OffchainState, url_param: String, response: Option<Vec<u8>>) {
    let mut url = "http://127.0.0.1:2020/eth/sign/".to_string();
    url.push_str(&url_param);

    state.expect_request(PendingRequest {
        method: "GET".into(),
        uri: url.into(),
        response,
        headers: vec![],
        sent: true,
        ..Default::default()
    });
}

#[allow(dead_code)]
pub fn mock_post_request(state: &mut OffchainState, body: Vec<u8>, response: Option<Vec<u8>>) {
    state.expect_request(PendingRequest {
        method: "POST".into(),
        uri: "http://127.0.0.1:2020/eth/send".into(),
        response,
        headers: vec![],
        body,
        sent: true,
        ..Default::default()
    });
}
