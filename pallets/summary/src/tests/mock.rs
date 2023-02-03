// Copyright 2022 Aventus Network Services (UK) Ltd.

pub use crate::{self as summary, *};
use avn::FinalisedBlockChecker;
use frame_support::{parameter_types, traits::GenesisBuild, BasicExternalities};
use frame_system as system;
use pallet_avn::{
    self as avn, testing::U64To32BytesConverter, vote::VotingSessionData, EthereumPublicKeyChecker,
};
use pallet_session as session;
use parking_lot::RwLock;
use sp_avn_common::{safe_add_block_numbers, safe_sub_block_numbers};
use sp_core::{
    ecdsa,
    offchain::{
        testing::{
            OffchainState, PendingRequest, PoolState, TestOffchainExt, TestTransactionPoolExt,
        },
        OffchainDbExt, OffchainWorkerExt, TransactionPoolExt,
    },
    H256,
};
use sp_runtime::{
    testing::{Header, TestSignature, TestXt, UintAuthorityId},
    traits::{BlakeTwo256, ConvertInto, IdentityLookup},
};
use sp_staking::{
    offence::{OffenceError, ReportOffence},
    SessionIndex,
};
use std::{cell::RefCell, convert::From, sync::Arc};

pub const APPROVE_ROOT: bool = true;
pub const REJECT_ROOT: bool = false;

pub type Extrinsic = TestXt<Call, ()>;

pub type AccountId = <TestRuntime as system::Config>::AccountId;
pub type BlockNumber = <TestRuntime as system::Config>::BlockNumber;

use pallet_ethereum_transactions::ethereum_transaction::TransactionId;

impl Summary {
    pub fn get_root_data(root_id: &RootId<BlockNumber>) -> RootData<AccountId> {
        return <<Summary as Store>::Roots>::get(root_id.range, root_id.ingress_counter)
    }

    pub fn insert_root_hash(
        root_id: &RootId<BlockNumber>,
        root_hash: H256,
        account_id: AccountId,
        tx_id: TransactionId,
    ) {
        <<Summary as Store>::Roots>::insert(
            root_id.range,
            root_id.ingress_counter,
            RootData::new(root_hash, account_id, Some(tx_id)),
        );
    }

    pub fn set_schedule_and_voting_periods(
        schedule_period: BlockNumber,
        voting_period: BlockNumber,
    ) {
        <<Summary as Store>::SchedulePeriod>::put(schedule_period);
        <<Summary as Store>::VotingPeriod>::put(voting_period);
    }

    pub fn set_root_as_validated(root_id: &RootId<BlockNumber>) {
        <<Summary as Store>::Roots>::mutate(root_id.range, root_id.ingress_counter, |root| {
            root.is_validated = true
        });
    }

    pub fn set_next_block_to_process(next_block_number_to_process: BlockNumber) {
        <<Summary as Store>::NextBlockToProcess>::put(next_block_number_to_process);
    }

    pub fn set_next_slot_block_number(slot_block_number: BlockNumber) {
        <<Summary as Store>::NextSlotAtBlock>::put(slot_block_number);
    }

    pub fn set_current_slot(slot: BlockNumber) {
        <<Summary as Store>::CurrentSlot>::put(slot);
    }

    pub fn set_current_slot_validator(validator_account: AccountId) {
        <<Summary as Store>::CurrentSlotsValidator>::put(validator_account);
    }

    pub fn set_previous_summary_slot(slot: BlockNumber) {
        <<Summary as Store>::SlotOfLastPublishedSummary>::put(slot);
    }

    pub fn get_block_number() -> BlockNumber {
        return System::block_number()
    }

    pub fn insert_pending_approval(root_id: &RootId<BlockNumber>) {
        <<Summary as Store>::PendingApproval>::insert(root_id.range, root_id.ingress_counter);
    }

    pub fn remove_pending_approval(root_range: &RootRange<BlockNumber>) {
        <<Summary as Store>::PendingApproval>::remove(root_range);
    }

    pub fn get_vote_for_root(
        root_id: &RootId<BlockNumber>,
    ) -> VotingSessionData<AccountId, BlockNumber> {
        <Summary as Store>::VotesRepository::get(root_id)
    }

    pub fn register_root_for_voting(
        root_id: &RootId<BlockNumber>,
        quorum: u32,
        voting_period_end: u64,
    ) {
        <<Summary as Store>::VotesRepository>::insert(
            root_id,
            VotingSessionData::new(root_id.encode(), quorum, voting_period_end, 0),
        );
    }

    pub fn deregister_root_for_voting(root_id: &RootId<BlockNumber>) {
        <<Summary as Store>::VotesRepository>::remove(root_id);
    }

    pub fn record_approve_vote(root_id: &RootId<BlockNumber>, voter: AccountId) {
        <<Summary as Store>::VotesRepository>::mutate(root_id, |vote| vote.ayes.push(voter));
    }

    pub fn record_reject_vote(root_id: &RootId<BlockNumber>, voter: AccountId) {
        <<Summary as Store>::VotesRepository>::mutate(root_id, |vote| vote.nays.push(voter));
    }

    pub fn set_total_ingresses(ingress_counter: IngressCounter) {
        <TotalIngresses<TestRuntime>>::put(ingress_counter);
    }

    pub fn emitted_event(event: &Event) -> bool {
        return System::events().iter().any(|a| a.event == *event)
    }

    pub fn emitted_event_for_offence_of_type(offence_type: SummaryOffenceType) -> bool {
        return System::events()
            .iter()
            .any(|e| Self::event_matches_offence_type(&e.event, offence_type.clone()))
    }

    pub fn event_matches_offence_type(event: &Event, this_type: SummaryOffenceType) -> bool {
        return matches!(event,
            mock::Event::Summary(
                crate::Event::<TestRuntime>::SummaryOffenceReported{ offence_type, .. }
            )
            if this_type == *offence_type
        )
    }

    pub fn total_events_emitted() -> usize {
        return System::events().len()
    }

    pub fn create_mock_identification_tuple(account_id: AccountId) -> (AccountId, AccountId) {
        return (account_id, account_id)
    }

    pub fn get_offence_record() -> Vec<(Vec<ValidatorId>, Offence)> {
        return OFFENCES.with(|o| o.borrow().to_vec())
    }

    pub fn reported_offence(
        reporter: AccountId,
        validator_count: u32,
        offenders: Vec<ValidatorId>,
        offence_type: SummaryOffenceType,
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

    pub fn reported_offence_of_type(offence_type: SummaryOffenceType) -> bool {
        let offences = Self::get_offence_record();

        return offences.iter().any(|o| Self::offence_is_of_type(o, offence_type.clone()))
    }

    fn offence_matches_criteria(
        this_report: &(Vec<ValidatorId>, Offence),
        these_reporters: Vec<ValidatorId>,
        this_count: u32,
        these_offenders: Vec<(ValidatorId, FullIdentification)>,
        this_type: SummaryOffenceType,
    ) -> bool {
        return matches!(
            this_report,
            (
                reporters,
                SummaryOffence {
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

    fn offence_is_of_type(
        this_report: &(Vec<ValidatorId>, Offence),
        this_type: SummaryOffenceType,
    ) -> bool {
        return matches!(
            this_report,
            (
                _,
                SummaryOffence {
                    session_index: _,
                    validator_set_count: _,
                    offenders: _,
                    offence_type}
            )
            if this_type == *offence_type
        )
    }
}

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
        Summary: summary::{Pallet, Call, Storage, Event<T>, Config<T>},
        Historical: pallet_session::historical::{Pallet, Storage},
    }
);

parameter_types! {
    pub const AdvanceSlotGracePeriod: u64 = 5;
    pub const MinBlockAge: u64 = 5;
    pub const FinalityReportLatency: u64 = 80;
}

pub type ValidatorId = u64;
type FullIdentification = u64;

pub const INITIAL_TRANSACTION_ID: TransactionId = 0;
pub const VALIDATOR_COUNT: u32 = 4;
thread_local! {
    // validator accounts (aka public addresses, public keys-ish)
    pub static VALIDATORS: RefCell<Option<Vec<ValidatorId>>> = RefCell::new(Some(vec![
        FIRST_VALIDATOR_INDEX,
        SECOND_VALIDATOR_INDEX,
        THIRD_VALIDATOR_INDEX,
        FOURTH_VALIDATOR_INDEX
    ]));

    static MOCK_TX_ID: RefCell<TransactionId> = RefCell::new(INITIAL_TRANSACTION_ID);

    static ETH_PUBLIC_KEY_VALID: RefCell<bool> = RefCell::new(true);

    static MOCK_RECOVERED_ACCOUNT_ID: RefCell<AccountId> = RefCell::new(FIRST_VALIDATOR_INDEX);
}

impl Config for TestRuntime {
    type Event = Event;
    type AdvanceSlotGracePeriod = AdvanceSlotGracePeriod;
    type MinBlockAge = MinBlockAge;
    type CandidateTransactionSubmitter = Self;
    type AccountToBytesConvert = U64To32BytesConverter;
    type ReportSummaryOffence = OffenceHandler;
    type FinalityReportLatency = FinalityReportLatency;
    type WeightInfo = ();
}

impl<LocalCall> system::offchain::SendTransactionTypes<LocalCall> for TestRuntime
where
    Call: From<LocalCall>,
{
    type OverarchingCall = Call;
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
    type Origin = Origin;
    type Call = Call;
    type Index = u64;
    type BlockNumber = u64;
    type Hash = H256;
    type Hashing = BlakeTwo256;
    type AccountId = u64;
    type Lookup = IdentityLookup<Self::AccountId>;
    type Header = Header;
    type Event = Event;
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
    type EthereumPublicKeyChecker = Self;
    type NewSessionHandler = ();
    type DisabledValidatorChecker = ();
    type FinalisedBlockChecker = Self;
}

parameter_types! {
    pub const Period: u64 = 1;
    pub const Offset: u64 = 0;
}

impl CandidateTransactionSubmitter<AccountId> for TestRuntime {
    fn submit_candidate_transaction_to_tier1(
        candidate_type: EthTransactionType,
        _tx_id: TransactionId,
        _submitter: AccountId,
        _signatures: Vec<ecdsa::Signature>,
    ) -> DispatchResult {
        if candidate_type !=
            EthTransactionType::PublishRoot(PublishRootData::new(
                ROOT_HASH_CAUSES_SUBMISSION_TO_T1_ERROR,
            ))
        {
            return Ok(())
        }
        Err(Error::<TestRuntime>::ErrorSubmitCandidateTxnToTier1.into())
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
}

/*********************** Add validators support ********************** */

pub struct TestSessionManager;
impl session::SessionManager<u64> for TestSessionManager {
    fn new_session(_new_index: SessionIndex) -> Option<Vec<ValidatorId>> {
        VALIDATORS.with(|l| l.borrow_mut().take())
    }
    fn end_session(_: SessionIndex) {}
    fn start_session(_: SessionIndex) {}
}

impl pallet_session::historical::Config for TestRuntime {
    type FullIdentification = u64;
    type FullIdentificationOf = ConvertInto;
}

impl pallet_session::historical::SessionManager<ValidatorId, FullIdentification>
    for TestSessionManager
{
    fn new_session(_new_index: SessionIndex) -> Option<Vec<(ValidatorId, FullIdentification)>> {
        VALIDATORS.with(|l| {
            l.borrow_mut()
                .take()
                .map(|validators| validators.iter().map(|v| (*v, *v)).collect())
        })
    }
    fn end_session(_: SessionIndex) {}
    fn start_session(_: SessionIndex) {}
}

type IdentificationTuple = (ValidatorId, FullIdentification);
type Offence = crate::SummaryOffence<IdentificationTuple>;

thread_local! {
    pub static OFFENCES: RefCell<Vec<(Vec<ValidatorId>, Offence)>> = RefCell::new(vec![]);
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

impl session::Config for TestRuntime {
    type SessionManager =
        pallet_session::historical::NoteHistoricalRoot<TestRuntime, TestSessionManager>;
    type Keys = UintAuthorityId;
    type ShouldEndSession = session::PeriodicSessions<Period, Offset>;
    type SessionHandler = (AVN,);
    type Event = Event;
    type ValidatorId = u64;
    type ValidatorIdOf = ConvertInto;
    type NextSessionRotation = session::PeriodicSessions<Period, Offset>;
    type WeightInfo = ();
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

    pub fn with_validators(mut self) -> Self {
        let validators: Vec<ValidatorId> = VALIDATORS.with(|l| l.borrow_mut().take().unwrap());
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

    pub fn with_validator_count(self, count: u64) -> Self {
        let validators_range: Vec<ValidatorId> = (1..=count).collect();
        VALIDATORS.with(|l| *l.borrow_mut() = Some(validators_range));

        return self.with_validators()
    }

    pub fn with_genesis_config(mut self) -> Self {
        let _ = summary::GenesisConfig::<TestRuntime> { schedule_period: 160, voting_period: 100 }
            .assimilate_storage(&mut self.storage);
        self
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

impl EthereumPublicKeyChecker<u64> for TestRuntime {
    fn get_validator_for_eth_public_key(_eth_public_key: &ecdsa::Public) -> Option<u64> {
        match ETH_PUBLIC_KEY_VALID.with(|pk| *pk.borrow()) {
            true => Some(MOCK_RECOVERED_ACCOUNT_ID.with(|pk| *pk.borrow())),
            _ => None,
        }
    }
}

impl FinalisedBlockChecker<BlockNumber> for TestRuntime {
    fn is_finalised(_block_number: BlockNumber) -> bool {
        true
    }
}

pub fn set_mock_recovered_account_id(account_id: AccountId) {
    MOCK_RECOVERED_ACCOUNT_ID.with(|acc_id| {
        *acc_id.borrow_mut() = account_id;
    });
}

/*********************** Mocking ********************** */

pub const ROOT_HASH_HEX_STRING: &'static [u8; 64] =
    b"8736c9e671fe581fe4ef4631112038297dcdecae163e8724c281ece8ad94c8c3";
pub const ROOT_HASH_BYTES: [u8; 32] = [
    135, 54, 201, 230, 113, 254, 88, 31, 228, 239, 70, 49, 17, 32, 56, 41, 125, 205, 236, 174, 22,
    62, 135, 36, 194, 129, 236, 232, 173, 148, 200, 195,
];
const ROOT_HASH_CAUSES_SUBMISSION_TO_T1_ERROR: [u8; 32] = [
    111, 54, 201, 230, 113, 254, 88, 31, 228, 239, 70, 49, 17, 32, 56, 41, 125, 205, 236, 174, 22,
    62, 135, 36, 194, 129, 236, 232, 173, 148, 200, 195,
];
pub const OTHER_CONTEXT: &'static [u8] = b"other_tx_context"; // TODO: Share it in a centralised avt suport pallet for testing
const CURRENT_BLOCK_NUMBER: u64 = 10;
const NEXT_BLOCK_TO_PROCESS: u64 = 3;
pub const CURRENT_SLOT: u64 = 5;
pub const VOTING_PERIOD_END: u64 = 12;
pub const QUORUM: u32 = 3;
pub const FIRST_VALIDATOR_INDEX: u64 = 1;
pub const SECOND_VALIDATOR_INDEX: u64 = 2;
pub const THIRD_VALIDATOR_INDEX: u64 = 3;
pub const FOURTH_VALIDATOR_INDEX: u64 = 4;
pub const VALIDATORS_COUNT: u64 = 4;
pub const DEFAULT_INGRESS_COUNTER: IngressCounter = 100;

#[derive(Clone)]
pub struct Context {
    pub current_block_number: u64,
    pub next_block_to_process: u64,
    pub last_block_in_range: u64,
    pub validator: Validator<UintAuthorityId, u64>,
    pub root_hash_h256: H256,
    pub root_hash_vec: Vec<u8>,
    pub url_param: String,
    pub sign_url_param: String,
    pub approval_signature: ecdsa::Signature,
    pub record_summary_calculation_signature: TestSignature,
    pub root_id: RootId<BlockNumber>,
    pub tx_id: TransactionId,
    pub current_slot: BlockNumber,
}

pub const DEFAULT_SCHEDULE_PERIOD: u64 = 2;
pub const DEFAULT_VOTING_PERIOD: u64 = 2;

pub fn setup_context() -> Context {
    Summary::set_schedule_and_voting_periods(DEFAULT_SCHEDULE_PERIOD, DEFAULT_VOTING_PERIOD);

    let current_block_number = CURRENT_BLOCK_NUMBER;
    let next_block_to_process = NEXT_BLOCK_TO_PROCESS;
    let last_block_in_range = next_block_to_process + Summary::schedule_period() - 1;
    let root_hash_h256 = H256::from(ROOT_HASH_BYTES);
    let root_hash_vec = ROOT_HASH_HEX_STRING.to_vec();
    let root_id = RootId::new(
        RootRange::new(next_block_to_process, last_block_in_range),
        DEFAULT_INGRESS_COUNTER,
    );
    let validator = get_validator(FIRST_VALIDATOR_INDEX);
    let approval_signature = ecdsa::Signature::try_from(&[1; 65][0..65]).unwrap();
    let tx_id = TestRuntime::reserve_transaction_id(&EthTransactionType::PublishRoot(
        PublishRootData::new(*root_hash_h256.as_fixed_bytes()),
    ))
    .unwrap();

    let data_to_sign = Summary::convert_data_to_eth_compatible_encoding(&RootData::<u64>::new(
        root_hash_h256.clone(),
        validator.account_id,
        Some(tx_id),
    ))
    .unwrap();

    Context {
        current_block_number,
        current_slot: CURRENT_SLOT,
        next_block_to_process,
        last_block_in_range,
        url_param: get_url_param(next_block_to_process),
        validator: validator.clone(),
        root_hash_h256,
        root_hash_vec,
        root_id,
        approval_signature,
        record_summary_calculation_signature: get_signature_for_record_summary_calculation(
            validator,
            UPDATE_BLOCK_NUMBER_CONTEXT,
            root_hash_h256,
            root_id.ingress_counter,
            last_block_in_range,
        ),
        tx_id,
        sign_url_param: data_to_sign,
    }
}

pub fn setup_blocks(context: &Context) {
    System::set_block_number(context.current_block_number);
    Summary::set_next_block_to_process(context.next_block_to_process);
    Summary::set_next_slot_block_number(context.current_block_number + Summary::schedule_period());
    Summary::set_current_slot_validator(context.validator.account_id);
}

pub fn setup_total_ingresses(context: &Context) {
    Summary::set_total_ingresses(context.root_id.ingress_counter - 1);
}

pub fn setup_voting(
    root_id: &RootId<BlockNumber>,
    root_hash_h256: H256,
    validator: &Validator<UintAuthorityId, u64>,
) {
    let tx_id: TransactionId = INITIAL_TRANSACTION_ID;
    Summary::insert_root_hash(root_id, root_hash_h256, validator.account_id.clone(), tx_id);
    Summary::insert_pending_approval(root_id);
    Summary::register_root_for_voting(root_id, QUORUM, VOTING_PERIOD_END);

    assert_eq!(Summary::get_vote(root_id).ayes.is_empty(), true);
    assert_eq!(Summary::get_vote(root_id).nays.is_empty(), true);
}

pub fn setup_voting_for_root_id(context: &Context) {
    setup_blocks(&context);
    setup_voting(&context.root_id, context.root_hash_h256, &context.validator);
}

pub fn mock_response_of_get_roothash(
    state: &mut OffchainState,
    url_param: String,
    response: Option<Vec<u8>>,
) {
    let mut url = "http://127.0.0.1:2020/roothash/".to_string();
    url.push_str(&url_param);

    state.expect_request(PendingRequest {
        method: "GET".into(),
        uri: url.into(),
        response,
        sent: true,
        ..Default::default()
    });
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

pub fn set_root_lock_with_expiry(
    block_number: BlockNumber,
    last_block_in_range: BlockNumber,
) -> bool {
    OcwLock::set_lock_with_expiry(
        block_number,
        OcwOperationExpiration::Fast,
        Summary::create_root_lock_name(last_block_in_range),
    )
    .is_ok()
}

pub fn set_vote_lock_with_expiry(block_number: BlockNumber, root_id: &RootId<BlockNumber>) -> bool {
    OcwLock::set_lock_with_expiry(
        block_number,
        OcwOperationExpiration::Fast,
        vote::create_vote_lock_name::<TestRuntime>(root_id),
    )
    .is_ok()
}

pub fn get_non_validator() -> Validator<UintAuthorityId, u64> {
    get_validator(10)
}

pub fn get_signature_for_approve_cast_vote(
    signer: &Validator<UintAuthorityId, u64>,
    context: &[u8],
    root_id: &RootId<BlockNumber>,
    eth_data_to_sign: &String,
    eth_signature: &ecdsa::Signature,
) -> TestSignature {
    signer
        .key
        .sign(
            &(
                context,
                root_id.encode(),
                APPROVE_ROOT,
                eth_data_to_sign.encode(),
                eth_signature.encode(),
            )
                .encode(),
        )
        .expect("Signature is signed")
}

pub fn get_signature_for_reject_cast_vote(
    validator: &Validator<UintAuthorityId, u64>,
    context: &[u8],
    root_id: &RootId<BlockNumber>,
) -> TestSignature {
    validator
        .key
        .sign(&(context, root_id.encode(), REJECT_ROOT).encode())
        .expect("Signature is signed")
}

pub fn get_url_param(next_block_to_process: u64) -> String {
    let mut url_param = next_block_to_process.to_string();
    url_param.push_str(&"/".to_string());

    let adjust = if next_block_to_process > 0 { 1 } else { 0 };
    let last_block = next_block_to_process + Summary::schedule_period() - adjust;
    url_param.push_str(&last_block.to_string());
    url_param
}

pub fn get_validator(index: u64) -> Validator<UintAuthorityId, u64> {
    Validator { account_id: index, key: UintAuthorityId(index) }
}

pub fn get_signature_for_record_summary_calculation(
    validator: Validator<UintAuthorityId, u64>,
    context: &[u8],
    root_hash_h256: H256,
    ingress_counter: IngressCounter,
    last_block_in_range: BlockNumber,
) -> TestSignature {
    validator
        .key
        .sign(&(context, root_hash_h256, ingress_counter, last_block_in_range).encode())
        .expect("Signature is signed")
}

pub fn get_root_hash_return_submit_to_tier1_fails() -> H256 {
    H256::from(ROOT_HASH_CAUSES_SUBMISSION_TO_T1_ERROR)
}

pub fn advance_block_numbers(number_of_blocks: u64) -> BlockNumber {
    let now = System::block_number().max(1);
    let new_block_number =
        safe_add_block_numbers(now, number_of_blocks).expect("Advanced block number is valid");
    System::set_block_number(new_block_number);
    return new_block_number
}

pub fn retreat_block_numbers(number_of_blocks: u64) -> BlockNumber {
    let now = System::block_number().max(1);
    let new_block_number =
        safe_sub_block_numbers(now, number_of_blocks).expect("Retreated block number is valid");
    System::set_block_number(new_block_number);
    return new_block_number
}
