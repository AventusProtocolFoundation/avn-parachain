// Copyright 2022 Aventus Network Services (UK) Ltd.

#![cfg(test)]

use crate::{self as pallet_avn, *};
use frame_support::{derive_impl, parameter_types, weights::Weight};
use frame_system::{self as system, DefaultConfig};
use hex_literal::hex;
use pallet_session as session;
use sp_core::{
    offchain::testing::{OffchainState, PendingRequest},
    H256,
};
use sp_runtime::{
    testing::UintAuthorityId,
    traits::{BlakeTwo256, ConvertInto, IdentityLookup},
    BuildStorage, Perbill,
};
use sp_state_machine::BasicExternalities;
use std::cell::RefCell;

pub type AccountId = <TestRuntime as system::Config>::AccountId;
pub type AVN = Pallet<TestRuntime>;

type Block = frame_system::mocking::MockBlock<TestRuntime>;

frame_support::construct_runtime!(
    pub enum TestRuntime
    {
        System: frame_system::{Pallet, Call, Config<T>, Storage, Event<T>},
        Session: pallet_session::{Pallet, Call, Storage, Event, Config<T>},
        Avn: pallet_avn::{Pallet, Storage, Event},
    }
);

impl Config for TestRuntime {
    type RuntimeEvent = RuntimeEvent;
    type AuthorityId = UintAuthorityId;
    type EthereumPublicKeyChecker = ();
    type NewSessionHandler = ();
    type DisabledValidatorChecker = ();
    type WeightInfo = ();
}

parameter_types! {
    pub const BlockHashCount: u64 = 250;
    pub const MaximumBlockWeight: Weight = Weight::from_parts(1024 as u64, 0);
    pub const MaximumBlockLength: u32 = 2 * 1024;
    pub const AvailableBlockRatio: Perbill = Perbill::from_percent(75);
    pub const ChallengePeriod: u64 = 2;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl system::Config for TestRuntime {
    type RuntimeOrigin = RuntimeOrigin;
    type Nonce = u64;
    type RuntimeCall = RuntimeCall;
    type Block = Block;
    type RuntimeEvent = RuntimeEvent;
    type BlockHashCount = BlockHashCount;
    type PalletInfo = PalletInfo;
}

parameter_types! {
    pub const Period: u64 = 1;
    pub const Offset: u64 = 0;
    pub const DisabledValidatorsThreshold: Perbill = Perbill::from_percent(33);
}

thread_local! {
    // validator accounts (aka public addresses, public keys-ish)
    pub static VALIDATORS: RefCell<Option<Vec<u64>>> = RefCell::new(Some(vec![1, 2, 3]));
}

pub type SessionIndex = u32;

pub static CUSTOM_BRIDGE_CONTRACT: H160 = H160(hex!("11111AAAAA22222BBBBB11111AAAAA22222BBBBB"));

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
    type RuntimeEvent = RuntimeEvent;
    type ValidatorId = u64;
    type ValidatorIdOf = ConvertInto;
    type NextSessionRotation = session::PeriodicSessions<Period, Offset>;
    type WeightInfo = ();
}

pub struct ExtBuilder {
    pub storage: sp_runtime::Storage,
}

impl ExtBuilder {
    pub fn build_default() -> Self {
        let storage = frame_system::GenesisConfig::<TestRuntime>::default()
            .build_storage()
            .unwrap()
            .into();
        Self { storage }
    }

    pub fn with_genesis_config(mut self) -> Self {
        let _ = pallet_avn::GenesisConfig::<TestRuntime> {
            _phantom: Default::default(),
            bridge_contract_address: H160::from(CUSTOM_BRIDGE_CONTRACT),
        }
        .assimilate_storage(&mut self.storage);
        self
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
