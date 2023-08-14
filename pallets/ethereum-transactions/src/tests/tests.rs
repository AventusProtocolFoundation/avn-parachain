#![cfg(test)]

use super::mock::*;
use crate::{ethereum_transaction::*, system::RawOrigin, *};
use frame_support::{assert_err, assert_noop, assert_ok};
use sp_core::{
    offchain::testing::{OffchainState, PendingRequest},
    H512,
};
use sp_runtime::{testing::TestSignature, traits::BadOrigin};
use std::convert::TryInto;

const SELECTED_SENDER_ACCOUNT_ID: AccountId = 1;
const OTHER_SENDER_ACCOUNT_ID: AccountId = 2;
const NON_VALIDATOR_ACCOUNT_ID: AccountId = 4;

const DEFAULT_HASH: [u8; 32] = [5; 32];
const DEFAULT_QUORUM: u32 = 3;
static ENCODED_DATA: &str = "c4024f0e030303030303030303030303030303030303030303030303030303030303030300000000000000000000000000\
0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000600000000000000000000000000\
0000000000000000000000000000000000000820404040404040404040404040404040404040404040404040404040404040404040404040404040404040404\
0404040404040404040404040404040404040404040505050505050505050505050505050505050505050505050505050505050505050505050505050505050\
505050505050505050505050505050505050505050505000000000000000000000000000000000000000000000000000000000000";

fn to_32_bytes(account: &AccountId) -> [u8; 32] {
    <mock::TestRuntime as Config>::AccountToBytesConvert::into_bytes(&account)
}

fn generate_valid_dispatched_transactions(
    count: usize,
    candidate_transaction: EthTransactionCandidate,
) -> (AccountId, Vec<EthTransactionCandidate>) {
    let mut results: Vec<EthTransactionCandidate> = vec![];
    let mut tx_ids: Vec<TransactionId> = vec![];
    for tx_id in 0..count {
        let mut candidate_tx = candidate_transaction.clone();
        candidate_tx.tx_id = tx_id.try_into().unwrap();
        tx_ids.push(candidate_tx.tx_id);
        EthereumTransactions::insert_to_repository(candidate_tx.clone());
        results.push(candidate_tx);
    }

    EthereumTransactions::insert_to_dispatched_avn_tx_ids(SELECTED_SENDER_ACCOUNT_ID, tx_ids);

    return (SELECTED_SENDER_ACCOUNT_ID, results)
}

fn generate_merkle_root_mock_data_with_signatures(
) -> (AccountId, [u8; 32], EthSignatures, EthTransactionCandidate) {
    let (from, root_hash, mut sigs, mut candidate_transaction) =
        generate_merkle_root_mock_data_with_sender();
    sigs.signatures_list.append(&mut create_mock_ecdsa_signatures());
    candidate_transaction.signatures = sigs.clone();
    return (from, root_hash, sigs, candidate_transaction)
}

fn generate_merkle_root_mock_data_with_sender(
) -> (AccountId, [u8; 32], EthSignatures, EthTransactionCandidate) {
    let (from, root_hash, sigs, mut candidate_transaction) =
        generate_merkle_root_mock_data_without_sender();
    candidate_transaction.from = Some(to_32_bytes(&from));
    return (from, root_hash, sigs, candidate_transaction)
}

fn generate_deregister_validator_mock_data_with_signatures() -> EthTransactionCandidate {
    let (mut sigs, mut candidate_transaction) =
        generate_deregister_validator_mock_data_with_sender();
    sigs.signatures_list.append(&mut create_mock_ecdsa_signatures());
    candidate_transaction.signatures = sigs.clone();
    return candidate_transaction
}

fn generate_deregister_validator_mock_data_with_sender() -> (EthSignatures, EthTransactionCandidate)
{
    let sigs = EthSignatures::new();
    let t1_public_key = [5u8; 64];
    let t2_public_key = [4u8; 32];
    let candidate_transaction = EthTransactionCandidate::new(
        EthereumTransactions::get_unique_transaction_identifier(),
        Some(to_32_bytes(&SELECTED_SENDER_ACCOUNT_ID)),
        EthTransactionType::DeregisterCollator(DeregisterCollatorData::new(
            H512::from(t1_public_key.clone()),
            t2_public_key.clone(),
        )),
        DEFAULT_QUORUM,
    );
    return (sigs, candidate_transaction)
}

fn generate_merkle_root_mock_data_without_sender(
) -> (AccountId, [u8; 32], EthSignatures, EthTransactionCandidate) {
    // TODO [TYPE: test refactoring][PRI: medium]: move this `from` to
    // `generate_merkle_root_mock_data_with_sender`
    let from: AccountId = SELECTED_SENDER_ACCOUNT_ID;
    let root_hash = [3u8; 32];
    let sigs = EthSignatures::new();

    let candidate_transaction = EthTransactionCandidate::new(
        EthereumTransactions::get_unique_transaction_identifier(),
        None,
        EthTransactionType::PublishRoot(PublishRootData::new(root_hash.clone())),
        DEFAULT_QUORUM,
    );

    return (from, root_hash, sigs, candidate_transaction)
}

// TODO: remove and replace by the method in the builder
fn create_mock_ecdsa_signatures() -> Vec<ecdsa::Signature> {
    vec![ecdsa::Signature::from_raw([4; 65]), ecdsa::Signature::from_raw([5; 65])]
}

// TODO [TYPE: test refactoring][PRI: medium] When we generate the signatures refactor this
fn generate_transaction_signature() -> TestSignature {
    TestSignature(0, vec![])
}

pub fn mock_send_tx_response(
    state: &mut OffchainState,
    body: EthTransaction,
    response: Option<Vec<u8>>,
) {
    state.expect_request(PendingRequest {
        method: "POST".into(),
        uri: "http://127.0.0.1:2020/eth/send".into(),
        response,
        headers: vec![],
        body: body.encode(),
        sent: true,
        ..Default::default()
    });
}

fn mark_avn_tx_as_sent(tx_id: TransactionId, eth_tx_hash: H256) {
    assert_ok!(<EthereumTransactions as Store>::Repository::mutate(tx_id, |tx| {
        tx.set_eth_tx_hash::<TestRuntime>(eth_tx_hash)
    }));
}

#[test]
fn validate_generated_publish_root_bytes_abi_success() {
    let mut ext = ExtBuilder::build_default().with_validators().as_externality();

    ext.execute_with(|| {
        let (_, _, _, candidate_tx) = generate_merkle_root_mock_data_with_signatures();
        let tx = candidate_tx.to_abi(AVN::<TestRuntime>::get_bridge_contract_address()).unwrap();

        assert_eq!(hex::encode(tx.data), ENCODED_DATA);
    });
}

// ========================== Test Module functions: public interface
// ===============================

// TODO: rename to TestSetupBuilder
// or move static methods to Mocks
struct DefaultTransactionBuilder {
    tx_type: EthTransactionType,
    sender: Option<AccountId>,
    confirmations: Vec<ecdsa::Signature>,
}

impl DefaultTransactionBuilder {
    fn new(tx_type: EthTransactionType) -> Self {
        Self { tx_type, sender: None, confirmations: vec![] }
    }

    pub fn default_roothash() -> [u8; 32] {
        return [3u8; 32]
    }

    // The quorum is 2/3 of the validators plus 1
    // Our validator list has 3 members, so the quorum must be 3
    // I chose not to implement the same logic of the quorum function
    // because that would defeat the point of the test being black box
    pub fn default_quorum() -> u32 {
        assert_eq!(mock::AVN::active_validators().len(), 3);
        3
    }

    pub fn build_default() -> Self {
        let tx_type = EthTransactionType::PublishRoot(PublishRootData {
            root_hash: Self::default_roothash(),
        });

        Self::new(tx_type)
    }

    pub fn with_sender(&mut self, sender: AccountId) -> &mut Self {
        self.sender = Some(sender);
        self
    }

    pub fn with_confirmations(&mut self) -> &mut Self {
        self.confirmations.append(&mut vec![
            ecdsa::Signature::from_raw([1; 65]),
            ecdsa::Signature::from_raw([2; 65]),
            ecdsa::Signature::from_raw([3; 65]),
        ]);
        self
    }

    pub fn as_candidate(&self, tx_id: TransactionId) -> EthTransactionCandidate {
        // Returns an EthTransactionCandidate that should be equal to what the Module produces when
        // submitting the tx_type The module obtains the tx_id by reading the nonce by
        // calling get_unique_transaction_identifier() If we call this here, then the module
        // will always generate a different tx_id Therefore, this function does not use the
        // same method, to avoid modifying the test state

        let sender = match self.sender {
            None => None,
            Some(account_id) => Some(to_32_bytes(&account_id)),
        };

        let mut tx_candidate = EthTransactionCandidate::new(
            tx_id,
            sender,
            self.tx_type.clone(),
            Self::default_quorum(),
        );
        tx_candidate.signatures.append(self.confirmations.clone());

        return tx_candidate
    }

    pub fn as_type(&self) -> EthTransactionType {
        return self.tx_type.clone()
    }
}

mod reserve_transaction_id {
    use super::*;

    mod updates_unique_identifier {
        use super::*;

        fn verify_value_incremented_by_1(old: TransactionId, new: TransactionId) {
            assert_eq!(old + 1, new);
        }

        #[test]
        fn fails_if_transaction_already_reserved() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let transaction = DefaultTransactionBuilder::build_default();
                let old_unique_identifier =
                    EthereumTransactions::get_current_unique_transaction_identifier();

                EthereumTransactions::insert_to_reservations(
                    &transaction.as_type(),
                    old_unique_identifier,
                );

                assert_err!(
                    EthereumTransactions::reserve_transaction_id(&transaction.as_type()),
                    Error::<TestRuntime>::TransactionExists
                );

                let new_unique_identifier =
                    EthereumTransactions::get_current_unique_transaction_identifier();

                assert_eq!(old_unique_identifier, new_unique_identifier);
            });
        }

        #[test]
        fn when_call_succeeds() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let transaction = DefaultTransactionBuilder::build_default();

                let old_unique_identifier =
                    EthereumTransactions::get_current_unique_transaction_identifier();

                assert_ok!(EthereumTransactions::reserve_transaction_id(&transaction.as_type()));

                let new_unique_identifier =
                    EthereumTransactions::get_current_unique_transaction_identifier();
                verify_value_incremented_by_1(old_unique_identifier, new_unique_identifier)
            });
        }
    }
}

struct Context {
    tx_id: TransactionId,
    transaction: DefaultTransactionBuilder,
    from: AccountId,
}

fn minimum_setup() -> Context {
    let eth_transaction = DefaultTransactionBuilder::build_default();
    let from = SELECTED_SENDER_ACCOUNT_ID;
    let tx_id = EthereumTransactions::reserve_transaction_id(&eth_transaction.as_type()).unwrap();

    Context { tx_id, transaction: eth_transaction, from }
}

fn call_submit_candidate_for_tier1(
    transaction: EthTransactionCandidate,
    sender: AccountId,
) -> DispatchResult {
    return EthereumTransactions::submit_candidate_transaction_to_tier1(
        transaction.call_data,
        transaction.tx_id,
        sender,
        transaction.signatures.signatures_list,
    )
}

fn get_expected_candidate(context: &mut Context) -> EthTransactionCandidate {
    return context
        .transaction
        .with_sender(context.from)
        .with_confirmations()
        .as_candidate(context.tx_id)
}

// ..................... submit_candidate_for_tier1 ........................
mod submit_candidate_for_tier1 {
    use super::*;

    mod fails_when {
        use super::*;

        #[test]
        fn transaction_exists_in_repository() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let mut context = minimum_setup();
                let expected_candidate = get_expected_candidate(&mut context);

                EthereumTransactions::insert_to_repository(expected_candidate.clone());

                assert_err!(
                    call_submit_candidate_for_tier1(expected_candidate, context.from),
                    Error::<TestRuntime>::TransactionExists
                );
            });
        }

        #[ignore]
        #[test]
        fn does_not_have_enough_confirmations() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let mut context = minimum_setup();

                assert_err!(
                    call_submit_candidate_for_tier1(
                        context.transaction.with_sender(context.from).as_candidate(context.tx_id),
                        context.from
                    ),
                    Error::<TestRuntime>::NotEnoughConfirmations
                );
            });
        }

        #[test]
        fn other_than_reserved_id_is_used() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let mut context = minimum_setup();
                // use different tx_id
                context.tx_id = EthereumTransactions::get_current_unique_transaction_identifier();
                let candidate = context
                    .transaction
                    .with_sender(context.from)
                    .with_confirmations()
                    .as_candidate(context.tx_id);

                assert_err!(
                    call_submit_candidate_for_tier1(candidate, context.from),
                    Error::<TestRuntime>::ReservedMismatch
                );
            });
        }
        #[test]
        fn if_not_reserved() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let mut candidate_transaction = DefaultTransactionBuilder::build_default();
                let from = SELECTED_SENDER_ACCOUNT_ID;
                // use unreserved tx_id
                let tx_id = EthereumTransactions::get_current_unique_transaction_identifier();
                let candidate = candidate_transaction
                    .with_sender(from)
                    .with_confirmations()
                    .as_candidate(tx_id);

                assert_err!(
                    call_submit_candidate_for_tier1(candidate, from),
                    Error::<TestRuntime>::ReservedMissing
                );
            });
        }
    }

    mod succeeds_and {
        use super::*;

        #[test]
        fn nonce_does_not_change() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let mut context = minimum_setup();
                let expected_candidate = get_expected_candidate(&mut context);

                let old_unique_identifier =
                    EthereumTransactions::get_current_unique_transaction_identifier();

                assert_ok!(call_submit_candidate_for_tier1(expected_candidate, context.from));
                assert_eq!(
                    old_unique_identifier,
                    EthereumTransactions::get_current_unique_transaction_identifier()
                );
            });
        }

        #[test]
        fn transaction_gets_submitted() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let mut context = minimum_setup();
                let expected_candidate = get_expected_candidate(&mut context);

                assert_ok!(call_submit_candidate_for_tier1(
                    expected_candidate.clone(),
                    context.from
                ));

                assert_eq!(
                    <EthereumTransactions>::get_transaction(context.tx_id),
                    expected_candidate
                );
            });
        }

        #[test]
        fn event_is_emitted() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let mut context = minimum_setup();
                let expected_candidate = get_expected_candidate(&mut context);

                assert_eq!(EthereumTransactions::event_count(), 0);

                assert_ok!(call_submit_candidate_for_tier1(expected_candidate, context.from));
                let event = mock::RuntimeEvent::EthereumTransactions(
                    crate::Event::<TestRuntime>::TransactionReadyToSend {
                        transaction_id: context.tx_id,
                        sender: context.from,
                    },
                );
                assert_eq!(EthereumTransactions::event_count(), 1);
                assert!(EthereumTransactions::event_emitted(&event));
            });
        }
    }
}

// ............. set_eth_tx_hash_for_dispatched_tx ........................

mod set_eth_tx_hash_for_dispatched_tx {
    use super::*;

    struct Context {
        tx_id: TransactionId,
        submitter: AccountId,
        hash: EthereumTransactionHash,
    }

    fn setup() -> Context {
        let mut eth_transaction = DefaultTransactionBuilder::build_default();
        let submitter = SELECTED_SENDER_ACCOUNT_ID;
        let tx_id = EthereumTransactions::get_current_unique_transaction_identifier();

        let candidate_transaction = eth_transaction.with_sender(submitter).as_candidate(tx_id);

        EthereumTransactions::insert_to_repository(candidate_transaction.clone());
        EthereumTransactions::insert_to_dispatched_avn_tx_ids(submitter, vec![tx_id]);

        Context { tx_id, submitter, hash: H256::from_slice(&DEFAULT_HASH) }
    }

    fn call_set_eth_tx_hash_for_dispatched_tx(
        origin: RuntimeOrigin,
        tx_id: TransactionId,
        submitter: AccountId,
        eth_tx_hash: EthereumTransactionHash,
    ) -> DispatchResult {
        let signature_for_unsigned_transaction = generate_transaction_signature();

        return EthereumTransactions::set_eth_tx_hash_for_dispatched_tx(
            origin,
            submitter,
            tx_id,
            eth_tx_hash,
            signature_for_unsigned_transaction,
        )
    }

    // Post-conditions
    mod success_implies {
        use super::*;

        #[test]
        fn tx_hash_is_updated() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let context = setup();

                let transaction = <EthereumTransactions>::get_transaction(&context.tx_id);
                assert_eq!(None, transaction.get_eth_tx_hash());

                assert_ok!(call_set_eth_tx_hash_for_dispatched_tx(
                    RawOrigin::None.into(),
                    context.tx_id,
                    context.submitter,
                    context.hash
                ));

                let transaction = <EthereumTransactions>::get_transaction(&context.tx_id);
                assert_eq!(Some(context.hash), transaction.get_eth_tx_hash());
            });
        }

        #[test]
        fn event_is_emitted() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let context = setup();

                assert_ok!(call_set_eth_tx_hash_for_dispatched_tx(
                    RawOrigin::None.into(),
                    context.tx_id,
                    context.submitter,
                    context.hash
                ));

                let event = mock::RuntimeEvent::EthereumTransactions(
                    crate::Event::<TestRuntime>::EthereumTransactionHashAdded {
                        transaction_id: context.tx_id,
                        transaction_hash: context.hash,
                    },
                );
                assert_eq!(EthereumTransactions::event_count(), 1);
                assert!(EthereumTransactions::event_emitted(&event));
            });
        }
    }

    mod fails_when {
        use super::*;

        // Bad parameters
        #[test]
        fn origin_is_signed() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let context = setup();

                // ensure this detects also that no changes have happened in the global state,
                // even though we called the transaction via this function and not directly
                // I have checked that the error is returned, but not that the state remains
                // untouched
                assert_noop!(
                    call_set_eth_tx_hash_for_dispatched_tx(
                        RuntimeOrigin::signed(Default::default()),
                        context.tx_id,
                        context.submitter,
                        context.hash,
                    ),
                    BadOrigin
                );
            });
        }

        #[test]
        fn submitter_is_not_a_validator() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let context = setup();

                assert_noop!(
                    call_set_eth_tx_hash_for_dispatched_tx(
                        RawOrigin::None.into(),
                        context.tx_id,
                        NON_VALIDATOR_ACCOUNT_ID,
                        context.hash,
                    ),
                    Error::<TestRuntime>::InvalidKey
                );
            });
        }

        #[test]
        fn submitter_is_not_the_designated_sender() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let context = setup();

                assert_noop!(
                    call_set_eth_tx_hash_for_dispatched_tx(
                        RawOrigin::None.into(),
                        context.tx_id,
                        OTHER_SENDER_ACCOUNT_ID,
                        context.hash,
                    ),
                    Error::<TestRuntime>::InvalidTransactionSubmitter
                );
            });
        }

        // Bad state
        #[test]
        fn submitter_is_not_registered() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let context = setup();

                EthereumTransactions::remove_submitter_from_dispatched_avn_tx_ids(
                    context.submitter,
                );

                assert_noop!(
                    call_set_eth_tx_hash_for_dispatched_tx(
                        RawOrigin::None.into(),
                        context.tx_id,
                        context.submitter,
                        context.hash,
                    ),
                    Error::<TestRuntime>::MissingDispatchedAvnTxSubmitter
                );
            });
        }

        #[test]
        fn submitter_is_not_registered_for_this_transaction() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let context = setup();

                EthereumTransactions::remove_single_tx_from_dispatched_avn_tx_ids(
                    context.submitter,
                    0,
                );

                assert_noop!(
                    call_set_eth_tx_hash_for_dispatched_tx(
                        RawOrigin::None.into(),
                        context.tx_id,
                        context.submitter,
                        context.hash,
                    ),
                    Error::<TestRuntime>::MissingDispatchedAvnTx
                );
            });
        }

        #[test]
        fn transaction_does_not_have_a_designated_submitter() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let context = setup();
                EthereumTransactions::reset_submitter(context.tx_id);

                assert_noop!(
                    call_set_eth_tx_hash_for_dispatched_tx(
                        RawOrigin::None.into(),
                        context.tx_id,
                        context.submitter,
                        context.hash,
                    ),
                    Error::<TestRuntime>::InvalidTransactionSubmitter
                );
            });
        }

        #[test]
        fn it_is_called_twice_for_same_transaction() {
            let mut ext = ExtBuilder::build_default().with_validators().as_externality();
            ext.execute_with(|| {
                let context = setup();

                assert_ok!(call_set_eth_tx_hash_for_dispatched_tx(
                    RawOrigin::None.into(),
                    context.tx_id,
                    context.submitter,
                    context.hash,
                ));

                assert_noop!(
                    call_set_eth_tx_hash_for_dispatched_tx(
                        RawOrigin::None.into(),
                        context.tx_id,
                        context.submitter,
                        context.hash,
                    ),
                    Error::<TestRuntime>::EthTransactionHashValueMutableOnce
                );
            });
        }
    }

    #[test]
    fn can_reset_tx_hash_to_zero() {
        let mut ext = ExtBuilder::build_default().with_validators().as_externality();
        ext.execute_with(|| {
            let context = setup();

            assert_ok!(call_set_eth_tx_hash_for_dispatched_tx(
                RawOrigin::None.into(),
                context.tx_id,
                context.submitter,
                context.hash
            ));

            assert_ok!(call_set_eth_tx_hash_for_dispatched_tx(
                RawOrigin::None.into(),
                context.tx_id,
                context.submitter,
                H256::zero(),
            ));
        });
    }
}

// -------------------------- other functions ---------------------------------

fn create_candidate_tx_test_data() -> EthTransactionCandidate {
    let root_hash = [3u8; 32];

    EthTransactionCandidate::new(
        1,
        None,
        EthTransactionType::PublishRoot(PublishRootData::new(root_hash)),
        DEFAULT_QUORUM,
    )
}

#[test]
fn eth_transaction_candidate_set_eth_tx_hash_success() {
    let mut test_tx = create_candidate_tx_test_data();
    let new_eth_tx_hash = H256::from([1u8; 32]);
    assert!(test_tx.set_eth_tx_hash::<TestRuntime>(new_eth_tx_hash).is_ok());
}

#[test]
fn eth_transaction_candidate_try_change_eth_tx_hash_after_set_fail() {
    let mut test_tx = create_candidate_tx_test_data();
    let new_eth_tx_hash = H256::from([1u8; 32]);
    let _ = test_tx.set_eth_tx_hash::<TestRuntime>(new_eth_tx_hash);
    assert!(test_tx.set_eth_tx_hash::<TestRuntime>(new_eth_tx_hash).is_err());
}

#[test]
fn eth_transaction_candidate_get_eth_tx_hash_zero_is_none() {
    let test_tx = create_candidate_tx_test_data();
    assert_eq!(test_tx.get_eth_tx_hash(), None)
}

#[test]
fn eth_transaction_candidate_get_eth_tx_hash_with_value() {
    let mut test_tx = create_candidate_tx_test_data();
    let new_eth_tx_hash = H256::from([1u8; 32]);
    let _ = test_tx.set_eth_tx_hash::<TestRuntime>(new_eth_tx_hash);
    assert_eq!(test_tx.get_eth_tx_hash(), Some(new_eth_tx_hash));
}

#[test]
fn tx_ready_to_be_sent_returns_correct_value_when_more_than_max_transactions_exist() {
    let (mut ext, _, _) = ExtBuilder::build_default()
        .with_validators()
        .for_offchain_worker()
        .as_externality_with_state();

    ext.execute_with(|| {
        let (_, _, _, candidate_transaction) = generate_merkle_root_mock_data_with_signatures();
        let (from, candidate_transactions) =
            generate_valid_dispatched_transactions(MAX_VALUES_RETURNED + 2, candidate_transaction);
        assert_eq!(candidate_transactions.len(), MAX_VALUES_RETURNED + 2);

        let txs_to_send = EthereumTransactions::transactions_ready_to_be_sent(&from);
        // We are limiting to 1 (MAX_VALUES_RETURNED) transactions per OCW
        assert_eq!(txs_to_send.len(), MAX_VALUES_RETURNED);
    });
}

#[test]
fn tx_ready_to_be_sent_returns_correct_value_when_exactly_max_transactions_exist() {
    let (mut ext, _, _) = ExtBuilder::build_default()
        .with_validators()
        .for_offchain_worker()
        .as_externality_with_state();

    ext.execute_with(|| {
        let (_, _, _, candidate_transaction) = generate_merkle_root_mock_data_with_signatures();
        let (from, candidate_transactions) =
            generate_valid_dispatched_transactions(MAX_VALUES_RETURNED, candidate_transaction);
        assert_eq!(candidate_transactions.len(), MAX_VALUES_RETURNED);

        let txs_to_send = EthereumTransactions::transactions_ready_to_be_sent(&from);
        assert_eq!(txs_to_send.len(), MAX_VALUES_RETURNED);
    });
}

#[test]
fn tx_ready_to_be_sent_returns_correct_value_when_fewer_than_max_transactions_exist() {
    let (mut ext, _, _) = ExtBuilder::build_default()
        .with_validators()
        .for_offchain_worker()
        .as_externality_with_state();

    ext.execute_with(|| {
        let dispatched_tx_count = MAX_VALUES_RETURNED - 1;
        let (_, _, _, candidate_transaction) = generate_merkle_root_mock_data_with_signatures();
        let (from, candidate_transactions) =
            generate_valid_dispatched_transactions(dispatched_tx_count, candidate_transaction);
        assert_eq!(candidate_transactions.len(), dispatched_tx_count);

        let txs_to_send = EthereumTransactions::transactions_ready_to_be_sent(&from);
        assert_eq!(txs_to_send.len(), dispatched_tx_count);
    });
}

#[test]
fn tx_ready_to_be_sent_returns_correct_value_none() {
    let mut ext = ExtBuilder::build_default().with_validators().as_externality();

    ext.execute_with(|| {
        let txs_to_send =
            EthereumTransactions::transactions_ready_to_be_sent(&SELECTED_SENDER_ACCOUNT_ID);
        assert_eq!(txs_to_send.len(), 0);
    });
}

fn get_expected_count_for_transactions_to_send(current: usize) -> usize {
    if MAX_VALUES_RETURNED < current {
        return MAX_VALUES_RETURNED
    } else {
        return current
    }
}

#[test]
fn tx_ready_to_be_sent_excludes_already_sent() {
    let (mut ext, _, _) = ExtBuilder::build_default()
        .with_validators()
        .for_offchain_worker()
        .as_externality_with_state();

    ext.execute_with(|| {
        let (_, _, _, candidate_transaction) = generate_merkle_root_mock_data_with_signatures();
        let count_of_tx_to_sent: usize = 2;
        let (from, candidate_transactions) =
            generate_valid_dispatched_transactions(count_of_tx_to_sent, candidate_transaction);
        assert_eq!(candidate_transactions.len(), count_of_tx_to_sent);

        let txs_to_send = EthereumTransactions::transactions_ready_to_be_sent(&from);
        assert_eq!(
            txs_to_send.len(),
            get_expected_count_for_transactions_to_send(count_of_tx_to_sent)
        );

        mark_avn_tx_as_sent(candidate_transactions[0].tx_id, H256::from([1u8; 32]));

        let txs_to_send = EthereumTransactions::transactions_ready_to_be_sent(&from);
        assert_eq!(txs_to_send.len(), 1);
    });
}

#[test]
fn tx_ready_to_be_sent_respects_locks() {
    let (mut ext, _, _) = ExtBuilder::build_default()
        .with_validators()
        .for_offchain_worker()
        .as_externality_with_state();

    ext.execute_with(|| {
        let (_, _, _, candidate_transaction) = generate_merkle_root_mock_data_with_signatures();
        let count_of_tx_to_sent: usize = 2;
        let (from, candidate_transactions) =
            generate_valid_dispatched_transactions(count_of_tx_to_sent, candidate_transaction);
        assert_eq!(candidate_transactions.len(), count_of_tx_to_sent);

        let txs_to_send = EthereumTransactions::transactions_ready_to_be_sent(&from);
        assert_eq!(
            txs_to_send.len(),
            get_expected_count_for_transactions_to_send(count_of_tx_to_sent)
        );

        // lock the first record (mark is as sent)
        assert_ok!(OcwLock::set_lock_with_expiry(
            1u64,
            OcwOperationExpiration::Custom(ETHEREUM_SEND_BLOCKS_EXPIRY),
            EthereumTransactions::generate_sending_lock_name(candidate_transactions[0].tx_id)
        ));

        let txs_to_send = EthereumTransactions::transactions_ready_to_be_sent(&from);
        assert_eq!(txs_to_send.len(), 1);
    });
}

#[test]
fn tx_ready_to_be_sent_only_includes_own_transactions() {
    let (mut ext, _, _) = ExtBuilder::build_default()
        .with_validators()
        .for_offchain_worker()
        .as_externality_with_state();

    ext.execute_with(|| {
        let (_, _, _, candidate_transaction) = generate_merkle_root_mock_data_with_signatures();
        let (from, candidate_transactions) =
            generate_valid_dispatched_transactions(1, candidate_transaction);
        assert_eq!(candidate_transactions.len(), 1);

        //Generate a valid candidate for a different account
        let other_selected_sender = SELECTED_SENDER_ACCOUNT_ID + 1;
        let (_, _, _, mut new_candidate_tx) = generate_merkle_root_mock_data_with_signatures();
        new_candidate_tx.from = Some(to_32_bytes(&other_selected_sender));
        EthereumTransactions::insert_to_repository(new_candidate_tx.clone());
        EthereumTransactions::insert_to_dispatched_avn_tx_ids(
            other_selected_sender,
            vec![new_candidate_tx.tx_id],
        );

        let txs_to_send = EthereumTransactions::transactions_ready_to_be_sent(&from);
        assert_eq!(txs_to_send.len(), 1);
        assert_eq!(txs_to_send[0].1.from, to_32_bytes(&SELECTED_SENDER_ACCOUNT_ID));

        let txs_to_send =
            EthereumTransactions::transactions_ready_to_be_sent(&other_selected_sender);
        assert_eq!(txs_to_send.len(), 1);
        assert_eq!(txs_to_send[0].1.from, to_32_bytes(&other_selected_sender));
    });
}

#[test]
fn eth_tx_hash_validates_correctly() {
    let (mut ext, _, offchain_state) =
        ExtBuilder::build_default().for_offchain_worker().as_externality_with_state();

    ext.execute_with(|| {
        let (_, _, _, candidate_transaction) = generate_merkle_root_mock_data_with_signatures();
        let (_, candidate_transactions) =
            generate_valid_dispatched_transactions(1, candidate_transaction);
        let body = candidate_transactions[0].to_abi(H160::random()).unwrap();
        let tx_hash = H256::random();
        let response = Some(hex::encode(tx_hash).as_bytes().to_vec());
        mock_send_tx_response(&mut offchain_state.write(), body.clone(), response);

        let result = EthereumTransactions::send_transaction_to_ethereum(body);
        assert!(result.unwrap() == tx_hash);
    });
}

#[test]
fn eth_tx_hash_invalid_hex_string() {
    let (mut ext, _, offchain_state) =
        ExtBuilder::build_default().for_offchain_worker().as_externality_with_state();

    ext.execute_with(|| {
        let invalid_tx_hash =
            "abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijkl".to_string();
        let (_, _, _, candidate_transaction) = generate_merkle_root_mock_data_with_signatures();
        let (_, candidate_transactions) =
            generate_valid_dispatched_transactions(1, candidate_transaction);
        let body = candidate_transactions[0].to_abi(H160::random()).unwrap();
        mock_send_tx_response(
            &mut offchain_state.write(),
            body.clone(),
            Some(invalid_tx_hash.as_bytes().to_vec()),
        );

        let result = EthereumTransactions::send_transaction_to_ethereum(body);
        assert_eq!(result, Err(Error::<TestRuntime>::InvalidHexString.into()));
    });
}

#[test]
fn eth_tx_hash_invalid_length() {
    let (mut ext, _, offchain_state) =
        ExtBuilder::build_default().for_offchain_worker().as_externality_with_state();

    ext.execute_with(|| {
        let invalid_tx_hash = H160::random();
        let (_, _, _, candidate_transaction) = generate_merkle_root_mock_data_with_signatures();
        let (_, candidate_transactions) =
            generate_valid_dispatched_transactions(1, candidate_transaction);
        let body = candidate_transactions[0].to_abi(H160::random()).unwrap();
        mock_send_tx_response(
            &mut offchain_state.write(),
            body.clone(),
            Some(hex::encode(invalid_tx_hash).as_bytes().to_vec()),
        );

        let result = EthereumTransactions::send_transaction_to_ethereum(body);
        assert_eq!(result, Err(Error::<TestRuntime>::InvalidHashLength.into()));
    });
}

#[test]
fn eth_tx_hash_invalid_bytes() {
    let (mut ext, _, offchain_state) =
        ExtBuilder::build_default().for_offchain_worker().as_externality_with_state();

    ext.execute_with(|| {
        let invalid_tx_hash = vec![159; 64];
        let (_, _, _, candidate_transaction) = generate_merkle_root_mock_data_with_signatures();
        let (_, candidate_transactions) =
            generate_valid_dispatched_transactions(1, candidate_transaction);
        let body = candidate_transactions[0].to_abi(H160::random()).unwrap();
        mock_send_tx_response(&mut offchain_state.write(), body.clone(), Some(invalid_tx_hash));

        let result = EthereumTransactions::send_transaction_to_ethereum(body);
        assert_eq!(result, Err(Error::<TestRuntime>::InvalidUTF8Bytes.into()));
    });
}

fn test_get_contract_address_for_eth_txn(
    transaction_type: &EthTransactionType,
    expected_contract_address: H160,
) {
    let actual_contract_address = Some(AVN::<TestRuntime>::get_bridge_contract_address());

    assert!(actual_contract_address.is_some(), "Contract address must not be empty");
    assert_eq!(actual_contract_address.unwrap(), expected_contract_address);
}

#[test]
fn eth_get_contract_address_works_with_valid_input() {
    let mut ext = ExtBuilder::build_default().as_externality();

    ext.execute_with(|| {
        EthereumTransactions::setup_mock_ethereum_contracts_address();
        let (_, _, _, publish_root_candidate_tx) = generate_merkle_root_mock_data_with_signatures();
        let (_, dispatched_candidate_txs) =
            generate_valid_dispatched_transactions(1, publish_root_candidate_tx);
        let publish_root_eth_tx_candidate = &dispatched_candidate_txs[0];
        let expected_contract_address = get_default_contract();
        test_get_contract_address_for_eth_txn(
            &publish_root_eth_tx_candidate.call_data,
            expected_contract_address,
        );

        let deregister_validator_candidate_tx =
            generate_deregister_validator_mock_data_with_signatures();
        let (_, dispatched_candidate_txs) =
            generate_valid_dispatched_transactions(1, deregister_validator_candidate_tx);
        let deregister_validator_eth_tx_candidate = &dispatched_candidate_txs[0];
        test_get_contract_address_for_eth_txn(
            &deregister_validator_eth_tx_candidate.call_data,
            H160::from(BRIDGE_CONTRACT),
        );
    });
}

mod unreserve_transaction_tests {
    use super::*;

    struct Context {
        tx_id: TransactionId,
        transaction_type: EthTransactionType,
    }
    impl Default for Context {
        fn default() -> Self {
            let root_hash = [1; 32];
            return Context {
                tx_id: 1,
                transaction_type: EthTransactionType::PublishRoot(PublishRootData::new(root_hash)),
            }
        }
    }
    impl Context {
        pub fn create_reservation(&self) {
            EthereumTransactions::insert_to_reservations(&self.transaction_type, self.tx_id);
        }
    }

    mod succeeds_when {
        use super::*;
        #[test]
        fn origin_is_root_and_value_exists() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                let context: Context = Default::default();
                context.create_reservation();

                assert_eq!(
                    true,
                    <ReservedTransactions<TestRuntime>>::contains_key(&context.transaction_type)
                );
                assert_ok!(EthereumTransactions::unreserve_transaction(
                    RawOrigin::Root.into(),
                    context.transaction_type.clone()
                ));
                assert_eq!(
                    false,
                    <ReservedTransactions<TestRuntime>>::contains_key(&context.transaction_type)
                );
                assert_eq!(
                    true,
                    <ReservedTransactions<TestRuntime>>::contains_key(
                        EthTransactionType::Discarded(context.tx_id)
                    )
                );
            });
        }

        #[test]
        fn origin_is_root_and_value_does_not_exist() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                let context: Context = Default::default();

                assert_eq!(
                    false,
                    <ReservedTransactions<TestRuntime>>::contains_key(&context.transaction_type)
                );
                assert_ok!(EthereumTransactions::unreserve_transaction(
                    RawOrigin::Root.into(),
                    context.transaction_type.clone()
                ));
                assert_eq!(
                    false,
                    <ReservedTransactions<TestRuntime>>::contains_key(&context.transaction_type)
                );
                assert_eq!(
                    false,
                    <ReservedTransactions<TestRuntime>>::contains_key(
                        EthTransactionType::Discarded(context.tx_id)
                    )
                );
            });
        }
    }

    mod fails_when {
        use super::*;
        #[test]
        fn origin_is_unsigned() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                let context: Context = Default::default();
                context.create_reservation();

                assert_eq!(
                    true,
                    <ReservedTransactions<TestRuntime>>::contains_key(&context.transaction_type)
                );
                assert_noop!(
                    EthereumTransactions::unreserve_transaction(
                        RawOrigin::None.into(),
                        context.transaction_type.clone()
                    ),
                    BadOrigin
                );
                assert_eq!(
                    true,
                    <ReservedTransactions<TestRuntime>>::contains_key(&context.transaction_type)
                );
                assert_eq!(
                    false,
                    <ReservedTransactions<TestRuntime>>::contains_key(
                        EthTransactionType::Discarded(context.tx_id)
                    )
                );
            });
        }

        #[test]
        fn origin_is_signed() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                let context: Context = Default::default();
                context.create_reservation();

                assert_eq!(
                    true,
                    <ReservedTransactions<TestRuntime>>::contains_key(&context.transaction_type)
                );
                assert_noop!(
                    EthereumTransactions::unreserve_transaction(
                        RuntimeOrigin::signed(Default::default()),
                        context.transaction_type.clone()
                    ),
                    BadOrigin
                );
                assert_eq!(
                    true,
                    <ReservedTransactions<TestRuntime>>::contains_key(&context.transaction_type)
                );
                assert_eq!(
                    false,
                    <ReservedTransactions<TestRuntime>>::contains_key(
                        EthTransactionType::Discarded(context.tx_id)
                    )
                );
            });
        }
    }
}
