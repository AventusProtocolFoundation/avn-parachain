//Copyright 2022 Aventus Network Services (UK) Ltd.

use crate::{self as validators_manager, *};
use avn::FinalisedBlockChecker;
use frame_support::{
    assert_ok, parameter_types,
    traits::{Currency, GenesisBuild, OnFinalize, OnInitialize},
    BasicExternalities, PalletId,
};
use hex_literal::hex;
use pallet_balances as balances;
use pallet_parachain_staking::{self as parachain_staking};

use pallet_timestamp as timestamp;
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
    sr25519, ByteArray, Pair, H256,
};
use sp_runtime::{
    testing::{Header, TestXt, UintAuthorityId},
    traits::{BlakeTwo256, ConvertInto, IdentityLookup, Verify},
};
use sp_staking::{
    offence::{OffenceError, ReportOffence},
    SessionIndex,
};

use codec::alloc::sync::Arc;
use parking_lot::RwLock;
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
pub type BlockNumber = <TestRuntime as system::Config>::BlockNumber;
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
    pub enum TestRuntime where
        Block = Block,
        NodeBlock = Block,
        UncheckedExtrinsic = UncheckedExtrinsic,
    {
        System: frame_system::{Pallet, Call, Config, Storage, Event<T>},
        ValidatorManager: validators_manager::{Pallet, Call, Storage, Event<T>, Config<T>},
        Session: pallet_session::{Pallet, Call, Storage, Event, Config<T>},
        Balances: pallet_balances::{Pallet, Call, Storage, Config<T>, Event<T>},
        AVN: pallet_avn::{Pallet, Storage},
        ParachainStaking: parachain_staking::{Pallet, Call, Storage, Config<T>, Event<T>},
    }
);

use frame_system as system;
use pallet_session as session;

impl ValidatorManager {
    pub fn insert_pending_approval(action_id: &ActionId<AccountId>) {
        <<ValidatorManager as Store>::PendingApprovals>::insert(
            action_id.action_account_id,
            action_id.ingress_counter,
        );
    }

    pub fn remove_pending_approval(action_id: &ActionId<AccountId>) {
        <<ValidatorManager as Store>::PendingApprovals>::remove(action_id.action_account_id);
    }

    pub fn get_voting_session_for_deregistration(
        action_id: &ActionId<AccountId>,
    ) -> VotingSessionData<AccountId, BlockNumber> {
        <ValidatorManager as Store>::VotesRepository::get(action_id)
    }

    pub fn create_voting_session(
        action_id: &ActionId<AccountId>,
        quorum: u32,
        voting_period_end: u64,
    ) {
        <<ValidatorManager as Store>::VotesRepository>::insert(
            action_id,
            VotingSessionData::new(action_id.encode(), quorum, voting_period_end, 0),
        );
    }

    pub fn insert_validators_action_data(
        action_id: &ActionId<AccountId>,
        reserved_eth_tx: EthTransactionType,
    ) {
        <<ValidatorManager as Store>::ValidatorActions>::insert(
            action_id.action_account_id,
            action_id.ingress_counter,
            ValidatorsActionData::new(
                ValidatorsActionStatus::AwaitingConfirmation,
                sender(),
                INITIAL_TRANSACTION_ID,
                ValidatorsActionType::Resignation,
                reserved_eth_tx,
            ),
        );
    }

    pub fn remove_voting_session(action_id: &ActionId<AccountId>) {
        <<ValidatorManager as Store>::VotesRepository>::remove(action_id);
    }

    pub fn record_approve_vote(action_id: &ActionId<AccountId>, voter: AccountId) {
        <<ValidatorManager as Store>::VotesRepository>::mutate(action_id, |vote| {
            vote.ayes.push(voter)
        });
    }

    pub fn record_reject_vote(action_id: &ActionId<AccountId>, voter: AccountId) {
        <<ValidatorManager as Store>::VotesRepository>::mutate(action_id, |vote| {
            vote.nays.push(voter)
        });
    }

    pub fn event_emitted(event: &RuntimeEvent) -> bool {
        return System::events().iter().any(|a| a.event == *event)
    }

    pub fn create_mock_identification_tuple(account_id: AccountId) -> (AccountId, AccountId) {
        return (account_id, account_id)
    }

    pub fn emitted_event_for_offence_of_type(offence_type: ValidatorOffenceType) -> bool {
        return System::events()
            .iter()
            .any(|e| Self::event_matches_offence_type(&e.event, offence_type.clone()))
    }

    pub fn event_matches_offence_type(
        event: &RuntimeEvent,
        this_type: ValidatorOffenceType,
    ) -> bool {
        return matches!(event,
            RuntimeEvent::ValidatorManager(
                crate::Event::<TestRuntime>::OffenceReported{ offence_type, offenders: _ }
            )
            if this_type == *offence_type
        )
    }

    pub fn get_offence_record() -> Vec<(Vec<ValidatorId>, Offence)> {
        return OFFENCES.with(|o| o.borrow().to_vec())
    }

    pub fn offence_reported(
        reporter: AccountId,
        validator_count: u32,
        offenders: Vec<ValidatorId>,
        offence_type: ValidatorOffenceType,
    ) -> bool {
        let offences = Self::get_offence_record();

        return offences.iter().any(|o| {
            Self::offence_matches_criteria(
                o,
                vec![reporter],
                validator_count,
                offenders.iter().map(|v| Self::create_mock_identification_tuple(*v)).collect(),
                offence_type.clone(),
            )
        })
    }

    fn offence_matches_criteria(
        this_report: &(Vec<ValidatorId>, Offence),
        these_reporters: Vec<ValidatorId>,
        this_count: u32,
        these_offenders: Vec<(ValidatorId, FullIdentification)>,
        this_type: ValidatorOffenceType,
    ) -> bool {
        return matches!(
            this_report,
            (
                reporters,
                ValidatorOffence {
                    session_index: _,
                    validator_set_count,
                    offenders,
                    offence_type}
            )
            if these_reporters == *reporters
            && this_count == *validator_set_count
            && these_offenders == *offenders
            && this_type == *offence_type
        )
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
    type CandidateTransactionSubmitter = Self;
    type ReportValidatorOffence = OffenceHandler;
    type ValidatorRegistrationNotifier = Self;
    type WeightInfo = ();
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
    type AccountData = balances::AccountData<u128>;
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type SystemWeightInfo = ();
    type SS58Prefix = ();
    type OnSetCode = ();
    type MaxConsumers = frame_support::traits::ConstU32<16>;
}

impl avn::Config for TestRuntime {
    type AuthorityId = UintAuthorityId;
    type EthereumPublicKeyChecker = Self;
    type NewSessionHandler = ValidatorManager;
    type DisabledValidatorChecker = ValidatorManager;
    type FinalisedBlockChecker = Self;
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

parameter_types! {
    pub const Period: u64 = 1;
    pub const Offset: u64 = 0;
}

impl CandidateTransactionSubmitter<AccountId> for TestRuntime {
    fn submit_candidate_transaction_to_tier1(
        _candidate_type: EthTransactionType,
        _tx_id: TransactionId,
        _submitter: AccountId,
        _signatures: Vec<ecdsa::Signature>,
    ) -> DispatchResult {
        Ok(())
    }

    fn reserve_transaction_id(
        _candidate_type: &EthTransactionType,
    ) -> Result<TransactionId, DispatchError> {
        let value = MOCK_TX_ID.with(|tx_id| *tx_id.borrow());
        MOCK_TX_ID.with(|tx_id| {
            *tx_id.borrow_mut() += 1;
        });
        return Ok(value)
    }
    #[cfg(feature = "runtime-benchmarks")]
    fn set_transaction_id(_candidate_type: &EthTransactionType, _id: TransactionId) {}
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
}

/// A mock offence report handler.
pub struct OffenceHandler;
impl ReportOffence<AccountId, IdentificationTuple, Offence> for OffenceHandler {
    fn report_offence(reporters: Vec<AccountId>, offence: Offence) -> Result<(), OffenceError> {
        OFFENCES.with(|l| l.borrow_mut().push((reporters, offence)));
        Ok(())
    }

    fn is_known_offence(_offenders: &[IdentificationTuple], _time_slot: &SessionIndex) -> bool {
        false
    }
}

impl FinalisedBlockChecker<BlockNumber> for TestRuntime {
    fn is_finalised(_block_number: BlockNumber) -> bool {
        true
    }
}

/// An extrinsic type used for tests.
type IdentificationTuple = (AccountId, AccountId);
type Offence = crate::ValidatorOffence<IdentificationTuple>;

pub const INITIAL_TRANSACTION_ID: TransactionId = 0;

thread_local! {
    static PROCESSED_EVENTS: RefCell<Vec<EthEventId>> = RefCell::new(vec![]);

    pub static VALIDATORS: RefCell<Option<Vec<AccountId>>> = RefCell::new(Some(vec![
        validator_id_1(),
        validator_id_2(),
        validator_id_3(),
        validator_id_4(),
        validator_id_5(),
    ]));

    static MOCK_TX_ID: RefCell<TransactionId> = RefCell::new(INITIAL_TRANSACTION_ID);

    pub static OFFENCES: RefCell<Vec<(Vec<AccountId>, Offence)>> = RefCell::new(vec![]);
}

impl ProcessedEventsChecker for TestRuntime {
    fn check_event(event_id: &EthEventId) -> bool {
        return PROCESSED_EVENTS.with(|l| l.borrow_mut().iter().any(|event| event == event_id))
    }
}

// TODO: Do we need to test the ECDSA sig verification logic here? If so, replace this with a call
// to the pallet's get_validator_for_eth_public_key method and update the tests to use "real"
// signatures
impl EthereumPublicKeyChecker<AccountId> for TestRuntime {
    fn get_validator_for_eth_public_key(eth_public_key: &ecdsa::Public) -> Option<AccountId> {
        if !<ValidatorManager as Store>::EthereumPublicKeys::contains_key(eth_public_key) {
            return None
        }
        return Some(<ValidatorManager as Store>::EthereumPublicKeys::get(eth_public_key).unwrap())
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
            frame_system::GenesisConfig::default().build_storage::<TestRuntime>().unwrap();
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
        ext.execute_with(|| frame_system::Pallet::<TestRuntime>::set_block_number(1u32.into()));
        ext
    }

    /// Setups a genesis configuration with 5 collators to the genesis state
    pub fn with_validators(mut self) -> Self {
        let validator_account_ids: &Vec<AccountId> =
            &VALIDATORS.with(|l| l.borrow().clone().unwrap());
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
                .zip(initial_validators_public_keys().iter().map(|pk| pk.clone()))
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
        ext.execute_with(|| frame_system::Pallet::<TestRuntime>::set_block_number(1u32.into()));
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
        assert_ok!(<ValidatorAccountIds<TestRuntime>>::try_append(to_insert.clone()));
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