use super::*;
use crate::{self as eth_bridge, request::add_new_send_request};
use avn;
use frame_support::parameter_types;
use sp_state_machine::BasicExternalities;

use frame_system as system;
use pallet_avn::{testing::U64To32BytesConverter, EthereumPublicKeyChecker};
use pallet_session as session;
use parking_lot::RwLock;
use sp_avn_common::{
    event_types::{EthEvent, EthEventId, LiftedData, ValidEvents},
    BridgeContractMethod,
};
use sp_core::{
    offchain::{
        testing::{OffchainState, PoolState, TestOffchainExt, TestTransactionPoolExt},
        OffchainDbExt, OffchainWorkerExt, TransactionPoolExt,
    },
    ConstU32, ConstU64, H256, U256,
};
use sp_runtime::{
    testing::{TestSignature, TestXt, UintAuthorityId},
    traits::{BlakeTwo256, ConvertInto, IdentityLookup},
    BuildStorage, DispatchError, DispatchResult, Perbill,
};
use sp_staking::offence::OffenceError;
use std::{cell::RefCell, convert::From, sync::Arc};

thread_local! {
    pub static OFFENCES: RefCell<Vec<(Vec<AccountId>, Offence)>> = RefCell::new(vec![]);
}

pub type Block = frame_system::mocking::MockBlock<TestRuntime>;
pub type Extrinsic = TestXt<RuntimeCall, ()>;
pub type AccountId = u64;
pub type BlockNumber = u64;

type IdentificationTuple = (AccountId, AccountId);
type Offence = crate::CorroborationOffence<IdentificationTuple>;
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
#[derive(Clone)]
pub struct Context {
    pub eth_tx_hash: H256,
    pub already_set_eth_tx_hash: H256,
    pub mock_event_partition: EthereumEventsPartition,
    pub bad_mock_event_partition: EthereumEventsPartition,
    pub second_mock_event_partition: EthereumEventsPartition,
    pub test_signature: TestSignature,
    pub test_signature_two: TestSignature,
    pub test_signature_three: TestSignature,
    pub confirmation_signature: ecdsa::Signature,
    pub tx_succeeded: bool,
    pub author: Author<TestRuntime>,
    pub author_two: Author<TestRuntime>,
    pub author_three: Author<TestRuntime>,
    pub confirming_author: Author<TestRuntime>,
    pub second_confirming_author: Author<TestRuntime>,
    pub third_confirming_author: Author<TestRuntime>,
    pub fourth_confirming_author: Author<TestRuntime>,
    pub request_function_name: Vec<u8>,
    pub request_params: Vec<(Vec<u8>, Vec<u8>)>,
    pub lower_params: LowerParams,
    pub finalised_block_vec: Option<Vec<u8>>,
    pub lower_id: u32,
    pub block_number: BlockNumber,
    pub expected_lower_msg_hash: String,
    pub eth_event_id: EthEventId,
    pub bad_eth_event_id: EthEventId,
}

const ROOT_HASH: &str = "30b83f0d722d1d4308ab4660a72dbaf0a7392d5674eca3cd21d57256d42df7a0";

frame_support::construct_runtime!(
    pub enum TestRuntime
    {
        System: frame_system::{Pallet, Call, Config<T>, Storage, Event<T>},
        Timestamp: pallet_timestamp,
        AVN: pallet_avn::{Pallet, Storage, Event},
        EthBridge: eth_bridge::{Pallet, Call, Storage, Event<T>, Config<T>},
        Session: pallet_session::{Pallet, Call, Storage, Event, Config<T>},
    }
);

impl<LocalCall> frame_system::offchain::SendTransactionTypes<LocalCall> for TestRuntime
where
    RuntimeCall: From<LocalCall>,
{
    type OverarchingCall = RuntimeCall;
    type Extrinsic = Extrinsic;
}

parameter_types! {
    pub const BlockHashCount: u64 = 250;
    pub const SS58Prefix: u8 = 42;
}

impl Config for TestRuntime {
    type MaxQueuedTxRequests = ConstU32<100>;
    type RuntimeEvent = RuntimeEvent;
    type TimeProvider = Timestamp;
    type RuntimeCall = RuntimeCall;
    type WeightInfo = ();
    type MinEthBlockConfirmation = ConstU64<20>;
    type AccountToBytesConvert = U64To32BytesConverter;
    type BridgeInterfaceNotification = TestRuntime;
    type ReportCorroborationOffence = OffenceHandler;
    type ProcessedEventsChecker = EthBridge;
    type EthereumEventsFilter = ();
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
    type MinimumPeriod = ConstU64<12000>;
    type WeightInfo = ();
}

impl avn::Config for TestRuntime {
    type AuthorityId = UintAuthorityId;
    type EthereumPublicKeyChecker = Self;
    type NewSessionHandler = ();
    type DisabledValidatorChecker = ();
    type WeightInfo = ();
    type RuntimeEvent = RuntimeEvent;
}

impl EthereumPublicKeyChecker<AccountId> for TestRuntime {
    fn get_validator_for_eth_public_key(_eth_public_key: &ecdsa::Public) -> Option<AccountId> {
        match ETH_PUBLIC_KEY_VALID.with(|pk| *pk.borrow()) {
            true => Some(MOCK_RECOVERED_ACCOUNT_ID.with(|pk| *pk.borrow())),
            _ => None,
        }
    }
}

fn generate_signature(author: Author<TestRuntime>, context: &[u8]) -> TestSignature {
    author.key.sign(&(context).encode()).expect("Signature is signed")
}

pub fn setup_eth_tx_request(context: &Context) -> EthereumId {
    add_new_send_request::<TestRuntime>(
        &context.request_function_name,
        &context.request_params,
        &vec![],
    )
    .unwrap()
}

pub fn create_confirming_author(author_id: u64) -> Author<TestRuntime> {
    Author::<TestRuntime> { key: UintAuthorityId(author_id), account_id: author_id }
}

pub fn create_mock_event_partition(events: EthEvent, part: u16) -> EthereumEventsPartition {
    let mut partition: BoundedBTreeSet<DiscoveredEvent, IncomingEventsBatchLimit> =
        BoundedBTreeSet::new();
    partition
        .try_insert(DiscoveredEvent { event: events.clone(), block: 2 })
        .unwrap();
    EthereumEventsPartition::new(
        EthBlockRange { start_block: 1, length: 1000 },
        part,
        false,
        partition,
    )
}

pub fn lower_is_ready_to_be_claimed(lower_id: &u32) -> bool {
    LOWERSREADYTOCLAIM.with(|lowers| lowers.borrow_mut().iter().any(|l| l == lower_id))
}

pub fn request_failed(id: &u32) -> bool {
    FAILEDREQUESTS.with(|reqs| reqs.borrow_mut().iter().any(|r| r == id))
}

pub fn setup_context() -> Context {
    let primary_validator_id = AVN::advance_primary_validator_for_sending().unwrap();
    let author = Author::<TestRuntime> {
        key: UintAuthorityId(primary_validator_id),
        account_id: primary_validator_id,
    };
    let author_two = Author::<TestRuntime> { key: UintAuthorityId(22), account_id: 22 };
    let author_three = Author::<TestRuntime> { key: UintAuthorityId(23), account_id: 23 };
    let mut confirming_validator_id: u64 = 1;
    if primary_validator_id == confirming_validator_id {
        confirming_validator_id += 1
    }
    let confirming_author = create_confirming_author(confirming_validator_id);
    let second_confirming_author = create_confirming_author(confirming_validator_id + 1);
    let third_confirming_author = create_confirming_author(confirming_validator_id + 2);
    let fourth_confirming_author = create_confirming_author(confirming_validator_id + 3);
    let test_signature = generate_signature(author.clone(), b"test context");
    let test_signature_two = generate_signature(author.clone(), b"test context");
    let test_signature_three = generate_signature(author.clone(), b"test context");
    let tx_succeeded = false;
    let eth_tx_hash = H256::from_slice(&[0u8; 32]);
    let already_set_eth_tx_hash = H256::from_slice(&[1u8; 32]);
    let confirmation_signature = ecdsa::Signature::try_from(&[1; 65][0..65]).unwrap();
    let finalised_block_vec = Some(hex::encode(10u32.encode()).into());
    let eth_event_id =
        EthEventId { signature: ValidEvents::Lifted.signature(), transaction_hash: eth_tx_hash };
    let bad_eth_event_id = EthEventId {
        signature: ValidEvents::Lifted.signature(),
        transaction_hash: H256::from_slice(&[6u8; 32]),
    };
    let bad_eth_event = EthEvent {
        event_id: bad_eth_event_id.clone(),
        event_data: sp_avn_common::event_types::EventData::LogLifted(LiftedData {
            token_contract: H160::zero(),
            sender_address: H160::zero(),
            receiver_address: H256::zero(),
            amount: 1,
            nonce: U256::zero(),
        }),
    };
    let mock_event_partition = create_mock_event_partition(
        EthEvent {
            event_id: eth_event_id.clone(),
            event_data: sp_avn_common::event_types::EventData::LogLifted(LiftedData {
                token_contract: H160::zero(),
                sender_address: H160::zero(),
                receiver_address: H256::zero(),
                amount: 1,
                nonce: U256::zero(),
            }),
        },
        0,
    );
    let bad_mock_event_partition = create_mock_event_partition(bad_eth_event, 0);

    let second_mock_event_partition = create_mock_event_partition(
        EthEvent {
            event_id: eth_event_id.clone(),
            event_data: sp_avn_common::event_types::EventData::LogLifted(LiftedData {
                token_contract: H160::zero(),
                sender_address: H160::zero(),
                receiver_address: H256::zero(),
                amount: 1,
                nonce: U256::zero(),
            }),
        },
        1,
    );

    UintAuthorityId::set_all_keys(vec![UintAuthorityId(primary_validator_id)]);

    Context {
        eth_tx_hash,
        mock_event_partition,
        second_mock_event_partition,
        already_set_eth_tx_hash,
        test_signature,
        test_signature_two,
        test_signature_three,
        tx_succeeded,
        author: author.clone(),
        author_two: author_two.clone(),
        author_three: author_three.clone(),
        confirming_author: confirming_author.clone(),
        second_confirming_author: second_confirming_author.clone(),
        third_confirming_author: third_confirming_author.clone(),
        fourth_confirming_author: fourth_confirming_author.clone(),
        confirmation_signature,
        request_function_name: BridgeContractMethod::PublishRoot.as_bytes().to_vec(),
        request_params: vec![(b"bytes32".to_vec(), hex::decode(ROOT_HASH).unwrap())],
        lower_params: [1u8; 76],
        finalised_block_vec,
        lower_id: 10u32,
        block_number: 1u64,
        // if request_params changes, this should also change
        expected_lower_msg_hash: "5892dee772ffe3d97e9525b62805bbcd91bac29026536cfa09269623128280ca"
            .to_string(),
        eth_event_id,
        bad_mock_event_partition,
        bad_eth_event_id,
    }
}

pub fn set_mock_recovered_account_id(account_id_bytes: [u8; 8]) {
    let account_id = AccountId::decode(&mut account_id_bytes.to_vec().as_slice()).unwrap();
    MOCK_RECOVERED_ACCOUNT_ID.with(|acc_id| {
        *acc_id.borrow_mut() = account_id;
    });
}

parameter_types! {
    pub const Period: u64 = 1;
    pub const Offset: u64 = 0;
    pub const DisabledValidatorsThreshold: Perbill = Perbill::from_percent(33);
}

thread_local! {
    // validator accounts (aka public addresses, public keys-ish)
    pub static VALIDATORS: RefCell<Option<Vec<u64>>> = RefCell::new(Some(vec![1, 2, 3, 4, 5, 6]));
    static ETH_PUBLIC_KEY_VALID: RefCell<bool> = RefCell::new(true);
    static MOCK_RECOVERED_ACCOUNT_ID: RefCell<AccountId> = RefCell::new(1);
    pub static LOWERSREADYTOCLAIM: RefCell<Vec<u32>> = RefCell::new(vec![]);
    pub static FAILEDREQUESTS: RefCell<Vec<u32>> = RefCell::new(vec![]);
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

impl pallet_session::historical::Config for TestRuntime {
    type FullIdentification = AccountId;
    type FullIdentificationOf = ConvertInto;
}

impl pallet_session::historical::SessionManager<AccountId, AccountId> for TestSessionManager {
    fn new_session(_new_index: SessionIndex) -> Option<Vec<(AccountId, AccountId)>> {
        VALIDATORS.with(|l| {
            l.borrow_mut()
                .take()
                .map(|validators| validators.iter().map(|v| (*v, *v)).collect())
        })
    }
    fn end_session(_: SessionIndex) {}
    fn start_session(_: SessionIndex) {}
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
            System::set_block_number(1)
        });
        ext
    }

    #[allow(dead_code)]
    pub fn with_genesis_config(mut self) -> Self {
        let _ = eth_bridge::GenesisConfig::<TestRuntime> {
            _phantom: Default::default(),
            eth_tx_lifetime_secs: 60 * 30,
            next_tx_id: 1,
            eth_block_range_size: 20u32,
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
            frame_system::Pallet::<TestRuntime>::set_block_number(1u32.into());
        });
        (ext, self.pool_state.unwrap(), self.offchain_state.unwrap())
    }
}

impl BridgeInterfaceNotification for TestRuntime {
    fn process_result(
        tx_id: EthereumId,
        _caller_id: Vec<u8>,
        tx_succeeded: bool,
    ) -> sp_runtime::DispatchResult {
        if !tx_succeeded {
            FAILEDREQUESTS.with(|l| l.borrow_mut().push(tx_id));
        }

        Ok(())
    }

    fn process_lower_proof_result(
        lower_id: u32,
        _caller_id: Vec<u8>,
        data: Result<Vec<u8>, ()>,
    ) -> sp_runtime::DispatchResult {
        if let Ok(_) = data {
            LOWERSREADYTOCLAIM.with(|l| l.borrow_mut().push(lower_id));
        } else {
            FAILEDREQUESTS.with(|l| l.borrow_mut().push(lower_id));
        }

        Ok(())
    }

    fn on_incoming_event_processed(event: &EthEvent) -> DispatchResult {
        if event.event_id.transaction_hash == H256::from_slice(&[6u8; 32]) {
            return Err(DispatchError::Other("Test - Bad event"))
        }

        Ok(())
    }
}
