//Copyright 2022 Aventus Network Services (UK) Ltd.

use crate::{self as validators_manager, *};
use frame_support::{
    parameter_types,
    traits::{Currency, GenesisBuild, OnFinalize, OnInitialize},
    BasicExternalities, PalletId,
};
use hex_literal::hex;
use pallet_balances as balances;
use pallet_eth_bridge::offence::CorroborationOffence;
use pallet_parachain_staking::{self as parachain_staking};

use pallet_avn::BridgeInterfaceNotification;
use pallet_timestamp as timestamp;
use parachain_staking::BlockNumberFor;
use sp_avn_common::{
    avn_tests_helpers::ethereum_converters::*,
    event_types::{AddedValidatorData, EthEvent, EthEventId, EventData, ValidEvents},
};
use sp_core::{
    ecdsa::Public,
    offchain::{
        testing::{
            OffchainState, PendingRequest, PoolState, TestOffchainExt, TestTransactionPoolExt,
        },
        OffchainDbExt, OffchainWorkerExt, TransactionPoolExt,
    },
    sr25519, ByteArray, ConstU64, Pair, H256,
};
use sp_runtime::{
    testing::{Header, TestXt, UintAuthorityId},
    traits::{BlakeTwo256, ConvertInto, IdentityLookup, Verify},
    BuildStorage,
};

use codec::alloc::sync::Arc;
use parking_lot::RwLock;
use sp_staking::{
    offence::{Offence, OffenceError, ReportOffence},
    SessionIndex,
};
use std::cell::RefCell;

pub fn validator_id_1() -> AccountId {
    TestAccount::new([1u8; 32]).account_id()
}

pub fn validator_id_2() -> AccountId {
    TestAccount::new([2u8; 32]).account_id()
}

pub fn validator_id_3() -> AccountId {
    TestAccount::new([3u8; 32]).account_id()
}

pub fn validator_id_4() -> AccountId {
    TestAccount::new([4u8; 32]).account_id()
}

pub fn validator_id_5() -> AccountId {
    TestAccount::new([5u8; 32]).account_id()
}

pub fn sender() -> AccountId {
    validator_id_3()
}

pub fn genesis_config_initial_validators() -> [AccountId; 5] {
    [validator_id_1(), validator_id_2(), validator_id_3(), validator_id_4(), validator_id_5()]
}
pub const REGISTERING_VALIDATOR_TIER1_ID: u128 = 200;
pub const EXISTENTIAL_DEPOSIT: u64 = 0;

pub type Extrinsic = TestXt<RuntimeCall, ()>;
pub type BlockNumber = BlockNumberFor<TestRuntime>;
pub type ValidatorId = <TestRuntime as session::Config>::ValidatorId;
pub type FullIdentification = AccountId;
/// The signature type used by accounts/transactions.
pub type Signature = sr25519::Signature;
/// An identifier for an account on this system.
pub type AccountId = <Signature as Verify>::Signer;

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<TestRuntime>;
type Block = frame_system::mocking::MockBlock<TestRuntime>;
// TODO: Refactor this struct to be reused in all tests
#[derive(Clone)]
pub struct TestAccount {
    pub seed: [u8; 32],
}

impl TestAccount {
    pub fn new(seed: [u8; 32]) -> Self {
        TestAccount { seed }
    }

    pub fn from_bytes(seed: &[u8]) -> Self {
        let mut seed_bytes: [u8; 32] = Default::default();
        seed_bytes.copy_from_slice(&seed[0..32]);
        TestAccount { seed: seed_bytes }
    }

    pub fn account_id(&self) -> AccountId {
        return AccountId::decode(&mut self.key_pair().public().to_vec().as_slice()).unwrap()
    }

    pub fn key_pair(&self) -> sr25519::Pair {
        return sr25519::Pair::from_seed(&self.seed)
    }
}

frame_support::construct_runtime!(
    pub enum TestRuntime
    {
        System: frame_system::{Pallet, Call, Config<T>, Storage, Event<T>},
        ValidatorManager: validators_manager::{Pallet, Call, Storage, Event<T>, Config<T>},
        Session: pallet_session::{Pallet, Call, Storage, Event, Config<T>},
        Balances: pallet_balances::{Pallet, Call, Storage, Config<T>, Event<T>},
        AVN: pallet_avn::{Pallet, Storage, Event},
        ParachainStaking: parachain_staking::{Pallet, Call, Storage, Config<T>, Event<T>},
        EthBridge: pallet_eth_bridge::{Pallet, Call, Storage, Event<T>},
        Timestamp: pallet_timestamp::{Pallet, Call, Storage, Inherent},
    }
);

use frame_system as system;
use pallet_session as session;

impl ValidatorManager {
    pub fn insert_validators_action_data(action_id: &ActionId<AccountId>) {
        <ValidatorActions<TestRuntime>>::insert(
            action_id.action_account_id,
            action_id.ingress_counter,
            ValidatorsActionData::new(
                ValidatorsActionStatus::AwaitingConfirmation,
                INITIAL_TRANSACTION_ID,
                ValidatorsActionType::Resignation,
            ),
        );
    }

    pub fn event_emitted(event: &RuntimeEvent) -> bool {
        return System::events().iter().any(|a| a.event == *event)
    }

    pub fn create_mock_identification_tuple(account_id: AccountId) -> (AccountId, AccountId) {
        return (account_id, account_id)
    }
}

parameter_types! {
    pub const VotingPeriod: u64 = 2;
}

impl Config for TestRuntime {
    type RuntimeEvent = RuntimeEvent;
    type ProcessedEventsChecker = Self;
    type VotingPeriod = VotingPeriod;
    type AccountToBytesConvert = AVN;
    type ValidatorRegistrationNotifier = Self;
    type WeightInfo = ();
    type BridgeInterface = EthBridge;
}

impl<LocalCall> system::offchain::SendTransactionTypes<LocalCall> for TestRuntime
where
    RuntimeCall: From<LocalCall>,
{
    type OverarchingCall = RuntimeCall;
    type Extrinsic = Extrinsic;
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
    type Nonce = u64;
    type Hash = H256;
    type Hashing = BlakeTwo256;
    type AccountId = AccountId;
    type Lookup = IdentityLookup<Self::AccountId>;
    type Block = Block;
    type RuntimeEvent = RuntimeEvent;
    type BlockHashCount = BlockHashCount;
    type Version = ();
    type PalletInfo = PalletInfo;
    type AccountData = balances::AccountData<u128>;
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type SystemWeightInfo = ();
    type SS58Prefix = ();
    type OnSetCode = ();
    type MaxConsumers = frame_support::traits::ConstU32<16>;
}

impl avn::Config for TestRuntime {
    type RuntimeEvent = RuntimeEvent;
    type AuthorityId = UintAuthorityId;
    type EthereumPublicKeyChecker = Self;
    type NewSessionHandler = ValidatorManager;
    type DisabledValidatorChecker = ValidatorManager;
    type WeightInfo = ();
}

parameter_types! {
    pub const ExistentialDeposit: u64 = EXISTENTIAL_DEPOSIT;
}

impl balances::Config for TestRuntime {
    type MaxLocks = frame_support::traits::ConstU32<1024>;
    type MaxReserves = ();
    type ReserveIdentifier = [u8; 8];
    type Balance = u128;
    type RuntimeEvent = RuntimeEvent;
    type DustRemoval = ();
    type ExistentialDeposit = ExistentialDeposit;
    type AccountStore = System;
    type WeightInfo = ();
    type RuntimeHoldReason = ();
    type FreezeIdentifier = ();
    type MaxHolds = ();
    type MaxFreezes = ();
}

parameter_types! {
    pub const MinimumPeriod: u64 = 3;
}

impl timestamp::Config for TestRuntime {
    type Moment = u64;
    type OnTimestampSet = ();
    type MinimumPeriod = MinimumPeriod;
    type WeightInfo = ();
}

impl pallet_eth_bridge::Config for TestRuntime {
    type MaxQueuedTxRequests = frame_support::traits::ConstU32<100>;
    type RuntimeEvent = RuntimeEvent;
    type TimeProvider = Timestamp;
    type MinEthBlockConfirmation = ConstU64<20>;
    type RuntimeCall = RuntimeCall;
    type WeightInfo = ();
    type AccountToBytesConvert = AVN;
    type BridgeInterfaceNotification = Self;
    type ReportCorroborationOffence = ();
}

impl BridgeInterfaceNotification for TestRuntime {
    fn process_result(
        _tx_id: u32,
        _caller_id: Vec<u8>,
        _tx_succeeded: bool,
    ) -> sp_runtime::DispatchResult {
        Ok(())
    }
}

parameter_types! {
    pub const Period: u64 = 1;
    pub const Offset: u64 = 0;
}

impl session::Config for TestRuntime {
    type SessionManager = ParachainStaking;
    type Keys = UintAuthorityId;
    type ShouldEndSession = ParachainStaking;
    type SessionHandler = (AVN,);
    type RuntimeEvent = RuntimeEvent;
    type ValidatorId = AccountId;
    type ValidatorIdOf = ConvertInto;
    type NextSessionRotation = ParachainStaking;
    type WeightInfo = ();
}

impl pallet_session::historical::Config for TestRuntime {
    type FullIdentification = AccountId;
    type FullIdentificationOf = ConvertInto;
}

parameter_types! {
    pub const MinBlocksPerEra: u32 = 2;
    pub const DefaultBlocksPerEra: u32 = 2;
    pub const MinSelectedCandidates: u32 = 20;
    pub const MaxTopNominationsPerCandidate: u32 = 4;
    pub const MaxBottomNominationsPerCandidate: u32 = 4;
    pub const MaxNominationsPerNominator: u32 = 4;
    pub const MinNominationPerCollator: u128 = 3;
    pub const ErasPerGrowthPeriod: u32 = 2;
    pub const RewardPaymentDelay: u32 = 2;
    pub const RewardPotId: PalletId = PalletId(*b"av/vamgr");
    pub const MaxCandidates: u32 = 256;
    pub const GrowthEnabled: bool = false;
}

impl parachain_staking::Config for TestRuntime {
    type RuntimeCall = RuntimeCall;
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type MinBlocksPerEra = MinBlocksPerEra;
    type RewardPaymentDelay = RewardPaymentDelay;
    type MinSelectedCandidates = MinSelectedCandidates;
    type MaxTopNominationsPerCandidate = MaxTopNominationsPerCandidate;
    type MaxBottomNominationsPerCandidate = MaxBottomNominationsPerCandidate;
    type MaxNominationsPerNominator = MaxNominationsPerNominator;
    type MinNominationPerCollator = MinNominationPerCollator;
    type RewardPotId = RewardPotId;
    type ErasPerGrowthPeriod = ErasPerGrowthPeriod;
    type Public = AccountId;
    type Signature = Signature;
    type CollatorSessionRegistration = Session;
    type CollatorPayoutDustHandler = ();
    type ProcessedEventsChecker = ();
    type WeightInfo = ();
    type MaxCandidates = MaxCandidates;
    type AccountToBytesConvert = AVN;
    type BridgeInterface = EthBridge;
    type GrowthEnabled = GrowthEnabled;
}

/// An extrinsic type used for tests.
type IdentificationTuple = (AccountId, AccountId);

pub const INITIAL_TRANSACTION_ID: EthereumTransactionId = 0;

thread_local! {
    static PROCESSED_EVENTS: RefCell<Vec<(H256,H256)>> = RefCell::new(vec![]);

    pub static VALIDATORS: RefCell<Option<Vec<AccountId>>> = RefCell::new(Some(vec![
        validator_id_1(),
        validator_id_2(),
        validator_id_3(),
        validator_id_4(),
        validator_id_5(),
    ]));

    static MOCK_TX_ID: RefCell<EthereumTransactionId> = RefCell::new(INITIAL_TRANSACTION_ID);
}

impl ProcessedEventsChecker for TestRuntime {
    fn check_event(event_id: &EthEventId) -> bool {
        return PROCESSED_EVENTS.with(|l| {
            l.borrow_mut().iter().any(|event| {
                &EthEventId { signature: event.0.clone(), transaction_hash: event.1.clone() } ==
                    event_id
            })
        })
    }
}

// TODO: Do we need to test the ECDSA sig verification logic here? If so, replace this with a call
// to the pallet's get_validator_for_eth_public_key method and update the tests to use "real"
// signatures
impl EthereumPublicKeyChecker<AccountId> for TestRuntime {
    fn get_validator_for_eth_public_key(eth_public_key: &ecdsa::Public) -> Option<AccountId> {
        if !EthereumPublicKeys::<TestRuntime>::contains_key(eth_public_key) {
            return None
        }
        return Some(EthereumPublicKeys::<TestRuntime>::get(eth_public_key).unwrap())
    }
}

impl ValidatorRegistrationNotifier<ValidatorId> for TestRuntime {
    fn on_validator_registration(_validator_id: &ValidatorId) {}
}

// Derived from [1u8;32] private key
pub(crate) const COLLATOR_1_ETHEREUM_PUPLIC_KEY: [u8; 33] =
    hex!["031b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f"];
// Derived from [2u8;32] private key
pub(crate) const COLLATOR_2_ETHEREUM_PUPLIC_KEY: [u8; 33] =
    hex!["024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d0766"];
// Derived from [3u8;32] private key

pub(crate) const COLLATOR_3_ETHEREUM_PUPLIC_KEY: [u8; 33] =
    hex!["02531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe337"];
// Derived from [4u8;32] private key

pub(crate) const COLLATOR_4_ETHEREUM_PUPLIC_KEY: [u8; 33] =
    hex!["03462779ad4aad39514614751a71085f2f10e1c7a593e4e030efb5b8721ce55b0b"];
// Derived from [5u8;32] private key

pub(crate) const COLLATOR_5_ETHEREUM_PUPLIC_KEY: [u8; 33] =
    hex!["0362c0a046dacce86ddd0343c6d3c7c79c2208ba0d9c9cf24a6d046d21d21f90f7"];

fn initial_validators_public_keys() -> Vec<ecdsa::Public> {
    return vec![
        Public::from_slice(&COLLATOR_1_ETHEREUM_PUPLIC_KEY).unwrap(),
        Public::from_slice(&COLLATOR_2_ETHEREUM_PUPLIC_KEY).unwrap(),
        Public::from_slice(&COLLATOR_3_ETHEREUM_PUPLIC_KEY).unwrap(),
        Public::from_slice(&COLLATOR_4_ETHEREUM_PUPLIC_KEY).unwrap(),
        Public::from_slice(&COLLATOR_5_ETHEREUM_PUPLIC_KEY).unwrap(),
    ]
}

fn initial_maximum_validators_public_keys() -> Vec<ecdsa::Public> {
    let mut public_keys = initial_validators_public_keys();

    for i in public_keys.len() as u32..<MaximumValidatorsBound as sp_core::TypedGet>::get() {
        public_keys.push(Public::from_raw([i as u8; 33]));
    }
    public_keys
}

pub struct ExtBuilder {
    pub storage: sp_runtime::Storage,
    offchain_state: Option<Arc<RwLock<OffchainState>>>,
    pool_state: Option<Arc<RwLock<PoolState>>>,
    txpool_extension: Option<TestTransactionPoolExt>,
    offchain_extension: Option<TestOffchainExt>,
    offchain_registered: bool,
}

impl ExtBuilder {
    pub fn build_default() -> Self {
        let storage =
            frame_system::GenesisConfig::<TestRuntime>::default().build_storage().unwrap();
        Self {
            storage,
            pool_state: None,
            offchain_state: None,
            txpool_extension: None,
            offchain_extension: None,
            offchain_registered: false,
        }
    }

    pub fn as_externality(self) -> sp_io::TestExternalities {
        let mut ext = sp_io::TestExternalities::from(self.storage);
        // Events do not get emitted on block 0, so we increment the block here
        ext.execute_with(|| {
            Timestamp::set_timestamp(1);
            frame_system::Pallet::<TestRuntime>::set_block_number(1u32.into())
        });
        ext
    }

    /// Setups a genesis configuration with 5 collators to the genesis state
    pub fn with_validators(self) -> Self {
        let validator_account_ids: &Vec<AccountId> =
            &VALIDATORS.with(|l| l.borrow().clone().unwrap());

        self.setup_validators(validator_account_ids, initial_validators_public_keys)
    }

    /// Setups a genesis configuration with maximum collators to the genesis state
    pub fn with_maximum_validators(self) -> Self {
        let mut validators_account_ids: Vec<AccountId> = vec![];
        // mock accounts
        for i in 1..=MaximumValidatorsBound::get() {
            let mut seed = [i as u8; 32];
            // [0u8;32] is the identity of the collator we add in the tests. Change the seed if its
            // the same.
            if seed.eq(&[0u8; 32]) {
                seed[30] = 1;
            }
            validators_account_ids.push(TestAccount::new(seed).account_id());
        }
        println!(
            "keys {:?} {:?}",
            validators_account_ids.len(),
            initial_maximum_validators_public_keys().len()
        );
        self.setup_validators(&validators_account_ids, initial_maximum_validators_public_keys)
    }

    /// Setups a genesis configuration with N collators to the genesis state
    fn setup_validators(
        mut self,
        validator_account_ids: &Vec<AccountId>,
        get_eth_keys: fn() -> Vec<ecdsa::Public>,
    ) -> Self {
        BasicExternalities::execute_with_storage(&mut self.storage, || {
            for ref k in validator_account_ids {
                frame_system::Pallet::<TestRuntime>::inc_providers(k);
            }
        });

        // Important: the order of the storage setup is important. Do not change it.
        let _ = pallet_balances::GenesisConfig::<TestRuntime> {
            balances: validator_account_ids.clone().into_iter().map(|v| (v, 10000)).collect(),
        }
        .assimilate_storage(&mut self.storage);

        let _ = session::GenesisConfig::<TestRuntime> {
            keys: validator_account_ids
                .clone()
                .into_iter()
                .enumerate()
                .map(|(i, v)| (v, v, UintAuthorityId((i as u32).into())))
                .collect(),
        }
        .assimilate_storage(&mut self.storage);

        let _ = parachain_staking::GenesisConfig::<TestRuntime> {
            candidates: validator_account_ids.clone().into_iter().map(|v| (v, 1000)).collect(),
            nominations: vec![],
            delay: 2,
            min_collator_stake: 10,
            min_total_nominator_stake: 5,
        }
        .assimilate_storage(&mut self.storage);

        let _ = validators_manager::GenesisConfig::<TestRuntime> {
            validators: validator_account_ids
                .iter()
                .map(|v| v.clone())
                .zip(get_eth_keys().iter().map(|pk| pk.clone()))
                .collect::<Vec<_>>(),
        }
        .assimilate_storage(&mut self.storage);

        self
    }

    pub fn with_validator_count(self, validators: Vec<AccountId>) -> Self {
        assert!(validators.len() <= initial_validators_public_keys().len());

        VALIDATORS.with(|l| *l.borrow_mut() = Some(validators));

        return self.with_validators()
    }

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

    pub fn as_externality_with_state(
        self,
    ) -> (sp_io::TestExternalities, Arc<RwLock<PoolState>>, Arc<RwLock<OffchainState>>) {
        assert!(self.offchain_registered);
        let mut ext = sp_io::TestExternalities::from(self.storage);
        ext.register_extension(OffchainDbExt::new(self.offchain_extension.clone().unwrap()));
        ext.register_extension(OffchainWorkerExt::new(self.offchain_extension.unwrap()));
        ext.register_extension(TransactionPoolExt::new(self.txpool_extension.unwrap()));
        assert!(self.pool_state.is_some());
        assert!(self.offchain_state.is_some());
        ext.execute_with(|| {
            Timestamp::set_timestamp(1);
            frame_system::Pallet::<TestRuntime>::set_block_number(1u32.into())
        });
        (ext, self.pool_state.unwrap(), self.offchain_state.unwrap())
    }
}

pub struct MockData {
    pub event: EthEvent,
    pub validator_data: AddedValidatorData,
    pub new_validator_id: AccountId,
    pub validator_eth_public_key: ecdsa::Public,
    pub collator_eth_public_key: ecdsa::Public,
}

impl MockData {
    pub fn setup_valid() -> Self {
        let event_id = EthEventId {
            signature: ValidEvents::AddedValidator.signature(),
            transaction_hash: H256::random(),
        };
        let data = Some(LogDataHelper::get_validator_data(REGISTERING_VALIDATOR_TIER1_ID));
        let topics = MockData::get_validator_token_topics();
        let validator_data = AddedValidatorData::parse_bytes(data.clone(), topics.clone()).unwrap();
        let collator_eth_public_key = ecdsa::Public::from_raw(hex!(
            "02407b0d9f41148bbe3b6c7d4a62585ae66cc32a707441197fa5453abfebd31d57"
        ));
        let new_validator_id =
            TestAccount::from_bytes(validator_data.t2_address.clone().as_bytes()).account_id();
        Balances::make_free_balance_be(&new_validator_id, 100000);
        MockData {
            validator_data: validator_data.clone(),
            event: EthEvent {
                event_data: EventData::LogAddedValidator(validator_data.clone()),
                event_id: event_id.clone(),
            },
            new_validator_id,
            validator_eth_public_key: ValidatorManager::compress_eth_public_key(
                validator_data.eth_public_key,
            ),
            collator_eth_public_key,
        }
    }

    pub fn get_validator_token_topics() -> Vec<Vec<u8>> {
        let topic_event_signature = LogDataHelper::get_topic_32_bytes(10);
        let topic_sender_lhs = LogDataHelper::get_topic_32_bytes(15);
        let topic_sender_rhs = LogDataHelper::get_topic_32_bytes(25);
        let topic_receiver = LogDataHelper::get_topic_32_bytes(30);
        return vec![topic_event_signature, topic_sender_lhs, topic_sender_rhs, topic_receiver]
    }
}

impl ValidatorManager {
    pub fn insert_to_validators(to_insert: &AccountId) {
        <ValidatorAccountIds<TestRuntime>>::try_append(to_insert.clone())
            .expect("Too many validator accounts in genesis");
    }
}

/// LogData Helper struct that converts values to topics and data
// TODO [TYPE: refactoring][PRI: low] We should consolidate the different versions of these
// functions and make one helper that can be used everywhere
pub struct LogDataHelper {}

impl LogDataHelper {
    pub fn get_validator_data(deposit: u128) -> Vec<u8> {
        return into_32_be_bytes(&deposit.to_le_bytes())
    }

    pub fn get_topic_32_bytes(n: u8) -> Vec<u8> {
        return vec![n; 32]
    }
}

// TODO [TYPE: test refactoring][PRI: low]: update this function to work with the mock builder
// pattern Currently, a straightforward replacement of the test setup leads to an error on the
// assert_eq!
pub fn advance_session() {
    let now = System::block_number().max(1);
    <crate::parachain_staking::ForceNewEra<TestRuntime>>::put(true);

    Balances::on_finalize(System::block_number());
    System::on_finalize(System::block_number());
    System::set_block_number(now + 1);
    System::on_initialize(System::block_number());
    Balances::on_initialize(System::block_number());
    Session::on_initialize(System::block_number());
    ParachainStaking::on_initialize(System::block_number());
}

pub fn mock_response_of_get_ecdsa_signature(
    state: &mut OffchainState,
    data_to_sign: String,
    response: Option<Vec<u8>>,
) {
    let mut url = "http://127.0.0.1:2020/eth/sign/".to_string();
    url.push_str(&data_to_sign);

    state.expect_request(PendingRequest {
        method: "GET".into(),
        uri: url.into(),
        response,
        sent: true,
        ..Default::default()
    });
}
