// This file is part of Aventus.
// Copyright (C) 2022 Aventus Network Services (UK) Ltd.

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

#![cfg(test)]

use frame_support::parameter_types;
use frame_system as system;
use sp_avn_common::event_types::EthEventId;
use sp_core::{sr25519, ConstU32, ConstU64, Pair, H256};
use sp_keystore::{testing::KeyStore, KeystoreExt};
use sp_runtime::{
    testing::Header,
    traits::{BlakeTwo256, IdentityLookup, Verify},
};
use std::cell::RefCell;

use crate::{self as nft_manager, *};
pub use std::sync::Arc;

/// The signature type used by accounts/transactions.
pub type Signature = sr25519::Signature;
/// An identifier for an account on this system.

pub type AccountId = <Signature as Verify>::Signer;
pub type Hashing = <TestRuntime as system::Config>::Hashing;

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<TestRuntime>;
type Block = frame_system::mocking::MockBlock<TestRuntime>;
pub type MockNftBatchBound = ConstU32<8>;

frame_support::construct_runtime!(
    pub enum TestRuntime where
        Block = Block,
        NodeBlock = Block,
        UncheckedExtrinsic = UncheckedExtrinsic,
    {
        System: frame_system::{Pallet, Call, Config, Storage, Event<T>},
        AVN: pallet_avn::{Pallet, Storage},
        NftManager: nft_manager::{Pallet, Call, Storage, Event<T>},
    }
);

impl Config for TestRuntime {
    type RuntimeEvent = mock::RuntimeEvent;
    type RuntimeCall = RuntimeCall;
    type ProcessedEventsChecker = Self;
    type Public = AccountId;
    type Signature = Signature;
    type WeightInfo = ();
    type BatchBound = MockNftBatchBound;
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
    type AccountId = AccountId;
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
    type MinimumPeriod = ConstU64<6000>;
    type WeightInfo = ();
}

impl avn::Config for TestRuntime {
    type AuthorityId = avn::sr25519::AuthorityId;
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
        let keystore = KeyStore::new();

        let mut ext = sp_io::TestExternalities::from(self.storage);
        ext.register_extension(KeystoreExt(Arc::new(keystore)));
        // Events do not get emitted on block 0, so we increment the block here
        ext.execute_with(|| System::set_block_number(1));
        ext
    }
}

thread_local! {
    static PROCESSED_EVENTS: RefCell<Vec<EthEventId>> = RefCell::new(vec![]);
}

pub fn insert_to_mock_processed_events(event_id: &EthEventId) {
    PROCESSED_EVENTS.with(|l| l.borrow_mut().push(event_id.clone()));
}

impl ProcessedEventsChecker for TestRuntime {
    fn check_event(event_id: &EthEventId) -> bool {
        return PROCESSED_EVENTS.with(|l| l.borrow_mut().iter().any(|event| event == event_id))
    }
}

pub struct TestAccount {
    pub seed: [u8; 32],
}

impl TestAccount {
    pub fn new(seed: [u8; 32]) -> Self {
        TestAccount { seed }
    }

    pub fn account_id(&self) -> AccountId {
        return AccountId::decode(&mut self.key_pair().public().to_vec().as_slice()).unwrap()
    }

    pub fn key_pair(&self) -> sr25519::Pair {
        return sr25519::Pair::from_seed(&self.seed)
    }
}

pub fn sign(signer: &sr25519::Pair, message_to_sign: &[u8]) -> Signature {
    return Signature::from(signer.sign(message_to_sign))
}
