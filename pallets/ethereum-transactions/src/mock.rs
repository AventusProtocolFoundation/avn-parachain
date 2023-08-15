// Copyright 2022 Aventus Network Services (UK) Ltd.

#![cfg(test)]

use frame_support::{parameter_types, traits::GenesisBuild, weights::Weight, BasicExternalities};
use hex_literal::hex;
use sp_core::{
    crypto::KeyTypeId,
    offchain::{
        testing::{OffchainState, PoolState, TestOffchainExt, TestTransactionPoolExt},
        OffchainDbExt as OffchainExt, OffchainWorkerExt, TransactionPoolExt,
    },
    H256,
};
use sp_io::TestExternalities;
use sp_runtime::{
    testing::{Header, TestXt, UintAuthorityId},
    traits::{BlakeTwo256, ConvertInto, IdentityLookup},
    Perbill,
};
use std::cell::RefCell;

use frame_system as system;

use codec::alloc::sync::Arc;
use pallet_session as session;
use parking_lot::RwLock;

use crate::{self as pallet_ethereum_transactions, *};
use pallet_avn::{testing::U64To32BytesConverter, FinalisedBlockChecker};

pub type AccountId = <TestRuntime as system::Config>::AccountId;
pub type AuthorityId = <TestRuntime as avn::Config>::AuthorityId;
pub type BlockNumber = <TestRuntime as system::Config>::BlockNumber;

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
        AVN: pallet_avn::{Pallet, Storage},
        EthereumTransactions: pallet_ethereum_transactions::{Pallet, Call, Storage, Event<T>, Config<T>},
    }
);

pub const KEY_TYPE: KeyTypeId = KeyTypeId(*b"ettx");
pub static CUSTOM_VALIDATOR_MANAGER_CONTRACT: H160 =
    H160(hex!("11111AAAAA22222BBBBB11111AAAAA22222BBBBB"));

pub mod crypto {
    use super::KEY_TYPE;
    use sp_runtime::app_crypto::{app_crypto, sr25519};
    app_crypto!(sr25519, KEY_TYPE);
}

parameter_types! {
    pub TestValidatorManagerContractAddress: H160 = CUSTOM_VALIDATOR_MANAGER_CONTRACT;
}

impl Config for TestRuntime {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeCall = RuntimeCall;
    type AccountToBytesConvert = U64To32BytesConverter;
    type ValidatorManagerContractAddress = TestValidatorManagerContractAddress;
    type WeightInfo = ();
}

parameter_types! {
    pub const BlockHashCount: u64 = 250;
    pub const MaximumBlockWeight: Weight = Weight::from_ref_time(1024);
    pub const MaximumBlockLength: u32 = 2 * 1024;
    pub const AvailableBlockRatio: Perbill = Perbill::from_percent(75);
    pub const ChallengePeriod: u64 = 2;
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

pub type Extrinsic = TestXt<RuntimeCall, ()>;

impl<LocalCall> frame_system::offchain::SendTransactionTypes<LocalCall> for TestRuntime
where
    RuntimeCall: From<LocalCall>,
{
    type OverarchingCall = RuntimeCall;
    type Extrinsic = Extrinsic;
}

parameter_types! {
    pub const Period: u64 = 1;
    pub const Offset: u64 = 0;
}

impl avn::Config for TestRuntime {
    type AuthorityId = UintAuthorityId;
    type EthereumPublicKeyChecker = ();
    type NewSessionHandler = ();
    type DisabledValidatorChecker = ();
    type FinalisedBlockChecker = Self;
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
    type RuntimeEvent = RuntimeEvent;
    type ValidatorId = u64;
    type ValidatorIdOf = ConvertInto;
    type NextSessionRotation = session::PeriodicSessions<Period, Offset>;
    type WeightInfo = ();
}

impl FinalisedBlockChecker<BlockNumber> for TestRuntime {
    fn is_finalised(_block_number: BlockNumber) -> bool {
        true
    }
}

impl EthereumTransactions {
    pub fn insert_to_repository(candidate_transaction: EthTransactionCandidate) {
        <EthereumTransactions as Store>::Repository::insert(
            candidate_transaction.tx_id,
            candidate_transaction,
        );
    }

    pub fn insert_to_reservations(candidate_type: &EthTransactionType, tx_id: TransactionId) {
        <EthereumTransactions as Store>::ReservedTransactions::insert(candidate_type, tx_id);
    }

    pub fn insert_to_dispatched_avn_tx_ids(
        submitter: AccountId,
        candidate_transaction_ids: Vec<TransactionId>,
    ) {
        let dispatched_data_vec: Vec<DispatchedData<TransactionId>> = candidate_transaction_ids
            .iter()
            .map(|id| DispatchedData::new(*id, 0u64))
            .collect();

        <EthereumTransactions as Store>::DispatchedAvnTxIds::insert(
            submitter,
            BoundedVec::truncate_from(dispatched_data_vec),
        );
    }

    pub fn remove_submitter_from_dispatched_avn_tx_ids(submitter: AccountId) {
        <EthereumTransactions as Store>::DispatchedAvnTxIds::remove(submitter);
    }

    pub fn remove_single_tx_from_dispatched_avn_tx_ids(submitter: AccountId, index: usize) {
        <EthereumTransactions as Store>::DispatchedAvnTxIds::mutate(submitter, |tx_list| {
            tx_list.remove(index)
        });
    }

    pub fn reset_submitter(tx_id: TransactionId) {
        <EthereumTransactions as Store>::Repository::mutate(tx_id, |storage_candidate| {
            storage_candidate.from = None
        });
    }

    // return the next available transaction identifier without changing it
    pub fn get_current_unique_transaction_identifier() -> u64 {
        return <EthereumTransactions as Store>::Nonce::get()
    }

    // TODO [TYPE: test refactoring][PRI: medium]: move this to a centralized pallet of test
    // utilities, when we do that refactoring, and apply it to all emitted system event
    // assertions.
    pub fn event_emitted(event: &RuntimeEvent) -> bool {
        return System::events().iter().any(|a| a.event == *event)
    }

    pub fn event_count() -> usize {
        return System::events().len()
    }

    pub fn get_validator_account_ids() -> Vec<AccountId> {
        return AVN::active_validators().iter().map(|v| v.account_id).collect()
    }
}

pub fn get_publish_root_default_contract() -> H160 {
    return H160::from([2u8; 20])
}

pub struct ExtBuilder {
    storage: sp_runtime::Storage,
    offchain_state: Option<Arc<RwLock<OffchainState>>>,
    pool_state: Option<Arc<RwLock<PoolState>>>,
    txpool_extension: Option<TestTransactionPoolExt>,
    offchain_extension: Option<TestOffchainExt>,
    offchain_registered: bool,
}

impl ExtBuilder {
    pub fn build_default() -> Self {
        let storage = pallet_ethereum_transactions::GenesisConfig::<TestRuntime>::default()
            .build_storage()
            .unwrap();
        Self {
            storage,
            pool_state: None,
            offchain_state: None,
            txpool_extension: None,
            offchain_extension: None,
            offchain_registered: false,
        }
    }

    pub fn with_genesis_config(mut self) -> Self {
        let _ = pallet_ethereum_transactions::GenesisConfig::<TestRuntime> {
            _phantom: Default::default(),
            get_publish_root_contract: get_publish_root_default_contract(),
        }
        .assimilate_storage(&mut self.storage);
        self
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

    // TODO [TYPE: test refactoring][PRI: medium]: Centralise these
    #[allow(dead_code)]
    pub fn for_offchain_worker(mut self) -> Self {
        assert!(!self.offchain_registered);
        let (offchain, offchain_state) = TestOffchainExt::new();
        let (pool, pool_state) = TestTransactionPoolExt::new();
        self.txpool_extension = Some(pool);
        self.offchain_extension = Some(offchain);
        self.pool_state = Some(pool_state);
        self.offchain_state = Some(offchain_state);
        self.offchain_registered = true;
        self
    }

    #[allow(dead_code)]
    pub fn as_externality_with_state(
        self,
    ) -> (TestExternalities, Arc<RwLock<PoolState>>, Arc<RwLock<OffchainState>>) {
        assert!(self.offchain_registered);
        let mut ext = sp_io::TestExternalities::from(self.storage);
        ext.register_extension(OffchainExt::new(self.offchain_extension.clone().unwrap()));
        ext.register_extension(OffchainWorkerExt::new(self.offchain_extension.unwrap()));
        ext.register_extension(TransactionPoolExt::new(self.txpool_extension.unwrap()));
        assert!(self.pool_state.is_some());
        assert!(self.offchain_state.is_some());
        ext.execute_with(|| System::set_block_number(1));
        (ext, self.pool_state.unwrap(), self.offchain_state.unwrap())
    }

    pub fn as_externality(self) -> sp_io::TestExternalities {
        let mut ext = sp_io::TestExternalities::from(self.storage);
        // Events do not get emitted on block 0, so we increment the block here
        ext.execute_with(|| System::set_block_number(1));
        ext
    }
}
