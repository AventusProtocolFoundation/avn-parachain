use crate::{
    mock::*, tests::RuntimeCall, CheckpointId, Error, Event, NextCheckpointId,
    REGISTER_CHAIN_HANDLER, SUBMIT_CHECKPOINT, UPDATE_CHAIN_HANDLER,
};
use codec::Encode;
use frame_support::{assert_noop, assert_ok, BoundedVec};
use pallet_avn_proxy::Error as avn_proxy_error;
use sp_avn_common::Proof;
use sp_core::{sr25519, ConstU32, Pair, H256};
use sp_runtime::{traits::Hash, DispatchError};

fn create_account_id(seed: u8) -> AccountId {
    create_account_pair(seed).public()
}

fn create_account_pair(seed: u8) -> sr25519::Pair {
    sr25519::Pair::from_seed(&[seed; 32])
}

fn bounded_vec(input: &[u8]) -> BoundedVec<u8, ConstU32<32>> {
    BoundedVec::<u8, ConstU32<32>>::try_from(input.to_vec()).unwrap()
}

fn create_proof(
    signer_pair: &sr25519::Pair,
    relayer: &AccountId,
    payload: &[u8],
) -> Proof<Signature, AccountId> {
    let signature = Signature::from(signer_pair.sign(payload));
    Proof { signer: signer_pair.public(), relayer: relayer.clone(), signature }
}

#[test]
fn register_chain_handler_works() {
    new_test_ext().execute_with(|| {
        let handler = create_account_id(1);
        let name = bounded_vec(b"Test Chain");

        assert_ok!(AvnAnchor::register_chain_handler(RuntimeOrigin::signed(handler), name.clone()));

        let chain_data = AvnAnchor::chain_handlers(handler).unwrap();
        assert_eq!(chain_data, 0);
        let chain_data = AvnAnchor::chain_data(0).unwrap();
        assert_eq!(chain_data.name, name);
        assert_eq!(AvnAnchor::nonces(chain_data.chain_id), 0);

        System::assert_last_event(Event::ChainHandlerRegistered(handler, 0, name).into());
    });
}

#[test]
fn register_chain_handler_fails_for_existing_handler() {
    new_test_ext().execute_with(|| {
        let handler = create_account_id(1);
        let name = bounded_vec(b"Test Chain");

        assert_ok!(AvnAnchor::register_chain_handler(RuntimeOrigin::signed(handler), name.clone()));

        assert_noop!(
            AvnAnchor::register_chain_handler(
                RuntimeOrigin::signed(handler),
                bounded_vec(b"Another Chain")
            ),
            Error::<TestRuntime>::HandlerAlreadyRegistered
        );
    });
}

#[test]
fn register_chain_handler_fails_for_empty_name() {
    new_test_ext().execute_with(|| {
        let handler = create_account_id(1);
        let empty_name = bounded_vec(b"");

        assert_noop!(
            AvnAnchor::register_chain_handler(RuntimeOrigin::signed(handler), empty_name),
            Error::<TestRuntime>::EmptyChainName
        );
    });
}

#[test]
fn register_chain_with_max_length_name_succeeds() {
    new_test_ext().execute_with(|| {
        let handler = create_account_id(1);
        let max_length_name = bounded_vec(&[b'a'; 32]);

        assert_ok!(AvnAnchor::register_chain_handler(
            RuntimeOrigin::signed(handler),
            max_length_name.clone()
        ));

        let chain_id = AvnAnchor::chain_handlers(handler).unwrap();
        let chain_data = AvnAnchor::chain_data(chain_id).unwrap();
        assert_eq!(chain_data.name, max_length_name);
    });
}

#[test]
fn update_chain_handler_works() {
    new_test_ext().execute_with(|| {
        let old_handler = create_account_id(1);
        let new_handler = create_account_id(2);
        let name = bounded_vec(b"Test Chain");

        assert_ok!(AvnAnchor::register_chain_handler(
            RuntimeOrigin::signed(old_handler),
            name.clone()
        ));
        assert_ok!(AvnAnchor::update_chain_handler(
            RuntimeOrigin::signed(old_handler),
            new_handler
        ));

        assert!(AvnAnchor::chain_handlers(old_handler).is_none());
        let chain_id = AvnAnchor::chain_handlers(new_handler).unwrap();
        let chain_data = AvnAnchor::chain_data(chain_id).unwrap();
        assert_eq!(chain_data.name, name);

        System::assert_last_event(
            Event::ChainHandlerUpdated(old_handler, new_handler, 0, name).into(),
        );
    });
}

#[test]
fn update_chain_handler_fails_for_non_existent_handler() {
    new_test_ext().execute_with(|| {
        let old_handler = create_account_id(1);
        let new_handler = create_account_id(2);

        assert_noop!(
            AvnAnchor::update_chain_handler(RuntimeOrigin::signed(old_handler), new_handler),
            Error::<TestRuntime>::ChainNotRegistered
        );
    });
}

#[test]
fn update_chain_handler_fails_for_already_registered_new_handler() {
    new_test_ext().execute_with(|| {
        let handler1 = create_account_id(1);
        let handler2 = create_account_id(2);

        assert_ok!(AvnAnchor::register_chain_handler(
            RuntimeOrigin::signed(handler1),
            bounded_vec(b"Chain 1")
        ));
        assert_ok!(AvnAnchor::register_chain_handler(
            RuntimeOrigin::signed(handler2),
            bounded_vec(b"Chain 2")
        ));

        assert_noop!(
            AvnAnchor::update_chain_handler(RuntimeOrigin::signed(handler1), handler2),
            Error::<TestRuntime>::HandlerAlreadyRegistered
        );
    });
}

#[test]
fn update_chain_handler_fails_for_non_handler() {
    new_test_ext().execute_with(|| {
        let current_handler = create_account_id(1);
        let new_handler = create_account_id(2);
        let unauthorized_account = create_account_id(3);
        let name = bounded_vec(b"Test Chain");

        assert_ok!(AvnAnchor::register_chain_handler(
            RuntimeOrigin::signed(current_handler),
            name.clone()
        ));

        assert_noop!(
            AvnAnchor::update_chain_handler(
                RuntimeOrigin::signed(unauthorized_account),
                new_handler
            ),
            Error::<TestRuntime>::ChainNotRegistered
        );

        let chain_id = AvnAnchor::chain_handlers(current_handler).unwrap();
        let chain_data = AvnAnchor::chain_data(chain_id).unwrap();
        assert_eq!(chain_data.chain_id, 0);
        assert_eq!(chain_data.name, name);

        assert_ok!(AvnAnchor::update_chain_handler(
            RuntimeOrigin::signed(current_handler),
            new_handler
        ));

        assert!(AvnAnchor::chain_handlers(current_handler).is_none());
        let updated_chain_id = AvnAnchor::chain_handlers(new_handler).unwrap();
        let updated_chain_data = AvnAnchor::chain_data(updated_chain_id).unwrap();
        assert_eq!(updated_chain_data.chain_id, 0);
        assert_eq!(updated_chain_data.name, name);

        System::assert_last_event(
            Event::ChainHandlerUpdated(current_handler, new_handler, 0, name).into(),
        );
    });
}

#[test]
fn submit_checkpoint_with_identity_works() {
    new_test_ext().execute_with(|| {
        let handler = create_account_id(1);
        let name = bounded_vec(b"Test Chain");
        let checkpoint = H256::random();

        assert_ok!(AvnAnchor::register_chain_handler(RuntimeOrigin::signed(handler), name));

        let chain_id = AvnAnchor::chain_handlers(handler).unwrap();
        let default_fee = DefaultCheckpointFee::get();

        assert_ok!(AvnAnchor::submit_checkpoint_with_identity(
            RuntimeOrigin::signed(handler),
            checkpoint
        ));

        System::assert_has_event(
            Event::CheckpointSubmitted(handler, chain_id, 0, checkpoint).into(),
        );
        System::assert_has_event(
            Event::CheckpointFeeCharged { handler, chain_id, fee: default_fee, nonce: 0 }.into(),
        );
    });
}

#[test]
fn submit_checkpoint_with_identity_fails_for_unregistered_handler() {
    new_test_ext().execute_with(|| {
        let handler = create_account_id(1);
        let checkpoint = H256::random();

        assert_noop!(
            AvnAnchor::submit_checkpoint_with_identity(RuntimeOrigin::signed(handler), checkpoint),
            Error::<TestRuntime>::ChainNotRegistered
        );
    });
}

#[test]
fn submit_multiple_checkpoints_increments_checkpoint_id() {
    new_test_ext().execute_with(|| {
        let handler = create_account_id(1);
        let name = bounded_vec(b"Test Chain");
        let checkpoint1 = H256::random();
        let checkpoint2 = H256::random();

        assert_ok!(AvnAnchor::register_chain_handler(RuntimeOrigin::signed(handler), name));
        assert_ok!(AvnAnchor::submit_checkpoint_with_identity(
            RuntimeOrigin::signed(handler),
            checkpoint1
        ));
        assert_ok!(AvnAnchor::submit_checkpoint_with_identity(
            RuntimeOrigin::signed(handler),
            checkpoint2
        ));

        assert_eq!(AvnAnchor::checkpoints(0, 0), checkpoint1);
        assert_eq!(AvnAnchor::checkpoints(0, 1), checkpoint2);
        assert_eq!(AvnAnchor::next_checkpoint_id(0), 2);

        System::assert_has_event(Event::CheckpointSubmitted(handler, 0, 0, checkpoint1).into());
        System::assert_has_event(Event::CheckpointSubmitted(handler, 0, 1, checkpoint2).into());
    });
}

#[test]
fn submit_checkpoints_for_multiple_chains() {
    new_test_ext().execute_with(|| {
        let handler1 = create_account_id(1);
        let handler2 = create_account_id(2);
        let name1 = bounded_vec(b"Chain 1");
        let name2 = bounded_vec(b"Chain 2");
        let checkpoint1 = H256::random();
        let checkpoint2 = H256::random();
        let checkpoint3 = H256::random();

        assert_ok!(AvnAnchor::register_chain_handler(RuntimeOrigin::signed(handler1), name1));
        assert_ok!(AvnAnchor::register_chain_handler(RuntimeOrigin::signed(handler2), name2));

        assert_ok!(AvnAnchor::submit_checkpoint_with_identity(
            RuntimeOrigin::signed(handler1),
            checkpoint1
        ));
        assert_ok!(AvnAnchor::submit_checkpoint_with_identity(
            RuntimeOrigin::signed(handler2),
            checkpoint2
        ));
        assert_ok!(AvnAnchor::submit_checkpoint_with_identity(
            RuntimeOrigin::signed(handler1),
            checkpoint3
        ));

        assert_eq!(AvnAnchor::checkpoints(0, 0), checkpoint1);
        assert_eq!(AvnAnchor::checkpoints(1, 0), checkpoint2);
        assert_eq!(AvnAnchor::checkpoints(0, 1), checkpoint3);

        assert_eq!(AvnAnchor::next_checkpoint_id(0), 2);
        assert_eq!(AvnAnchor::next_checkpoint_id(1), 1);

        System::assert_has_event(Event::CheckpointSubmitted(handler1, 0, 0, checkpoint1).into());
        System::assert_has_event(Event::CheckpointSubmitted(handler2, 1, 0, checkpoint2).into());
        System::assert_has_event(Event::CheckpointSubmitted(handler1, 0, 1, checkpoint3).into());
    });
}

#[test]
fn register_multiple_chains_increments_chain_id() {
    new_test_ext().execute_with(|| {
        let handler1 = create_account_id(1);
        let handler2 = create_account_id(2);
        let name1 = bounded_vec(b"Chain 1");
        let name2 = bounded_vec(b"Chain 2");

        assert_ok!(AvnAnchor::register_chain_handler(
            RuntimeOrigin::signed(handler1),
            name1.clone()
        ));
        assert_ok!(AvnAnchor::register_chain_handler(
            RuntimeOrigin::signed(handler2),
            name2.clone()
        ));

        let chain_id1 = AvnAnchor::chain_handlers(handler1).unwrap();
        let chain_id2 = AvnAnchor::chain_handlers(handler2).unwrap();

        let chain_data1 = AvnAnchor::chain_data(chain_id1).unwrap();
        let chain_data2 = AvnAnchor::chain_data(chain_id2).unwrap();

        assert_eq!(chain_data1.chain_id, 0);
        assert_eq!(chain_data1.name, name1);
        assert_eq!(chain_data2.chain_id, 1);
        assert_eq!(chain_data2.name, name2);

        System::assert_has_event(Event::ChainHandlerRegistered(handler1, 0, name1).into());
        System::assert_has_event(Event::ChainHandlerRegistered(handler2, 1, name2).into());
    });
}

#[test]
fn proxy_signed_register_chain_handler_works() {
    new_test_ext().execute_with(|| {
        let handler_pair = create_account_pair(1);
        let handler_account = handler_pair.public();
        let relayer = create_account_id(2);
        let name = bounded_vec(b"Test Chain");

        let initial_chain_id = AvnAnchor::next_chain_id();
        let payload =
            (REGISTER_CHAIN_HANDLER, relayer.clone(), handler_account.clone(), name.clone())
                .encode();
        let proof = create_proof(&handler_pair, &relayer, &payload);

        let call = Box::new(RuntimeCall::AvnAnchor(
            super::Call::<TestRuntime>::signed_register_chain_handler {
                proof,
                handler: handler_account.clone(),
                name: name.clone(),
            },
        ));

        assert_ok!(AvnProxy::proxy(RuntimeOrigin::signed(relayer.clone()), call.clone(), None));

        let chain_id =
            AvnAnchor::chain_handlers(handler_account.clone()).expect("Chain data should exist");
        assert_eq!(chain_id, initial_chain_id, "Chain ID mismatch");
        let chain_data = AvnAnchor::chain_data(chain_id).expect("Chain data not found");
        assert_eq!(chain_data.name, name, "Chain name mismatch");

        System::assert_has_event(
            Event::ChainHandlerRegistered(handler_account.clone(), initial_chain_id, name).into(),
        );

        assert!(
            proxy_event_emitted(
                relayer.clone(),
                <TestRuntime as frame_system::Config>::Hashing::hash_of(&call)
            ),
            "Proxy event should be emitted"
        );
        assert_eq!(
            AvnAnchor::nonces(initial_chain_id),
            0,
            "Nonce should be 0 for a newly registered chain"
        );
    });
}

#[test]
fn signed_update_chain_handler_works() {
    new_test_ext().execute_with(|| {
        let old_handler_pair = create_account_pair(1);
        let old_handler = old_handler_pair.public();
        let new_handler = create_account_id(2);
        let relayer = create_account_id(3);
        let name = bounded_vec(b"Test Chain");

        assert_ok!(AvnAnchor::register_chain_handler(
            RuntimeOrigin::signed(old_handler),
            name.clone()
        ));

        let chain_id = AvnAnchor::chain_handlers(old_handler).unwrap();
        let nonce = AvnAnchor::nonces(chain_id);
        let payload = (
            UPDATE_CHAIN_HANDLER,
            relayer.clone(),
            old_handler.clone(),
            new_handler.clone(),
            chain_id,
            nonce,
        )
            .encode();
        let proof = create_proof(&old_handler_pair, &relayer, &payload);

        let call = Box::new(RuntimeCall::AvnAnchor(
            super::Call::<TestRuntime>::signed_update_chain_handler {
                proof: proof.clone(),
                old_handler: old_handler.clone(),
                new_handler: new_handler.clone(),
            },
        ));

        assert_ok!(AvnProxy::proxy(RuntimeOrigin::signed(relayer.clone()), call.clone(), None));

        assert!(AvnAnchor::chain_handlers(old_handler).is_none());
        let updated_chain_id = AvnAnchor::chain_handlers(new_handler).unwrap();
        assert_eq!(updated_chain_id, chain_id);

        assert!(proxy_event_emitted(
            relayer.clone(),
            <TestRuntime as frame_system::Config>::Hashing::hash_of(&call)
        ));
    });
}

#[test]
fn signed_submit_checkpoint_with_identity_works() {
    new_test_ext().execute_with(|| {
        let handler_pair = create_account_pair(1);
        let handler = handler_pair.public();
        let relayer = create_account_id(2);
        let name = bounded_vec(b"Test Chain");
        let checkpoint = H256::random();

        assert_ok!(AvnAnchor::register_chain_handler(RuntimeOrigin::signed(handler), name));

        let chain_id = AvnAnchor::chain_handlers(handler).unwrap();
        let nonce = AvnAnchor::nonces(chain_id);
        let payload =
            (SUBMIT_CHECKPOINT, relayer.clone(), handler.clone(), checkpoint, chain_id, nonce)
                .encode();
        let proof = create_proof(&handler_pair, &relayer, &payload);

        let call = Box::new(RuntimeCall::AvnAnchor(
            super::Call::<TestRuntime>::signed_submit_checkpoint_with_identity {
                proof: proof.clone(),
                handler: handler.clone(),
                checkpoint,
            },
        ));

        assert_ok!(AvnProxy::proxy(RuntimeOrigin::signed(relayer.clone()), call.clone(), None));

        assert_eq!(AvnAnchor::checkpoints(chain_id, 0), checkpoint);
        assert_eq!(AvnAnchor::next_checkpoint_id(chain_id), 1);

        assert!(proxy_event_emitted(
            relayer.clone(),
            <TestRuntime as frame_system::Config>::Hashing::hash_of(&call)
        ));
    });
}

#[test]
fn proxy_signed_register_chain_handler_fails_with_wrong_relayer() {
    new_test_ext().execute_with(|| {
        let handler_pair = create_account_pair(1);
        let handler = handler_pair.public();
        let relayer = create_account_id(2);
        let wrong_relayer = create_account_id(3);
        let name = bounded_vec(b"Test Chain");

        let nonce = 0;
        let payload =
            (REGISTER_CHAIN_HANDLER, relayer.clone(), handler.clone(), name.clone(), nonce)
                .encode();
        let proof = create_proof(&handler_pair, &relayer, &payload);

        let call = Box::new(RuntimeCall::AvnAnchor(
            super::Call::<TestRuntime>::signed_register_chain_handler {
                proof: proof.clone(),
                handler: handler.clone(),
                name: name.clone(),
            },
        ));

        assert_noop!(
            AvnProxy::proxy(RuntimeOrigin::signed(wrong_relayer), call.clone(), None),
            avn_proxy_error::<TestRuntime>::UnauthorizedProxyTransaction
        );
    });
}

#[test]
fn proxy_signed_update_chain_handler_fails_with_invalid_signature() {
    new_test_ext().execute_with(|| {
        let old_handler_pair = create_account_pair(1);
        let old_handler = old_handler_pair.public();
        let new_handler = create_account_id(2);
        let relayer = create_account_id(3);
        let name = bounded_vec(b"Test Chain");

        assert_ok!(AvnAnchor::register_chain_handler(
            RuntimeOrigin::signed(old_handler.clone()),
            name.clone()
        ));

        let invalid_payload = b"invalid payload";
        let invalid_signature = old_handler_pair.sign(invalid_payload);

        let proof = Proof {
            signer: old_handler.clone(),
            relayer: relayer.clone(),
            signature: invalid_signature,
        };

        let call = Box::new(RuntimeCall::AvnAnchor(
            super::Call::<TestRuntime>::signed_update_chain_handler {
                proof,
                old_handler: old_handler.clone(),
                new_handler: new_handler.clone(),
            },
        ));

        assert_ok!(AvnProxy::proxy(RuntimeOrigin::signed(relayer.clone()), call.clone(), None),);

        assert_eq!(
            true,
            inner_call_failed_event_emitted(
                avn_proxy_error::<TestRuntime>::UnauthorizedProxyTransaction.into()
            )
        );
    });
}

#[test]
fn proxy_signed_submit_checkpoint_with_identity_fails_with_unregistered_handler() {
    new_test_ext().execute_with(|| {
        let registered_handler = create_account_id(1);
        let name = bounded_vec(b"Test Chain");
        assert_ok!(AvnAnchor::register_chain_handler(
            RuntimeOrigin::signed(registered_handler),
            name
        ));

        let unauthorized_handler_pair = create_account_pair(2);
        let unauthorized_handler = unauthorized_handler_pair.public();
        let relayer = create_account_id(3);
        let checkpoint = H256::random();

        let chain_id = 0;
        let nonce: u64 = AvnAnchor::nonces(chain_id);
        let payload = (
            SUBMIT_CHECKPOINT,
            relayer.clone(),
            unauthorized_handler.clone(),
            checkpoint,
            chain_id,
            nonce,
        )
            .encode();
        let proof = create_proof(&unauthorized_handler_pair, &relayer, &payload);

        let call = Box::new(RuntimeCall::AvnAnchor(
            super::Call::<TestRuntime>::signed_submit_checkpoint_with_identity {
                proof,
                handler: unauthorized_handler.clone(),
                checkpoint,
            },
        ));

        assert_ok!(AvnProxy::proxy(RuntimeOrigin::signed(relayer.clone()), call.clone(), None));

        assert!(inner_call_failed_event_emitted(
            avn_proxy_error::<TestRuntime>::UnauthorizedProxyTransaction.into()
        ));
    });
}

#[test]
fn checkpoint_id_overflow_fails() {
    new_test_ext().execute_with(|| {
        let handler = create_account_id(1);
        let name = bounded_vec(b"Test Chain");
        let checkpoint = H256::random();

        assert_ok!(AvnAnchor::register_chain_handler(RuntimeOrigin::signed(handler), name));

        NextCheckpointId::<TestRuntime>::insert(0, CheckpointId::MAX);

        assert_noop!(
            AvnAnchor::submit_checkpoint_with_identity(RuntimeOrigin::signed(handler), checkpoint),
            Error::<TestRuntime>::NoAvailableCheckpointId
        );
    });
}

// Fees
#[test]
fn set_checkpoint_fee_works() {
    new_test_ext().execute_with(|| {
        let chain_id = 0;
        let new_fee = 100;

        assert_ok!(AvnAnchor::set_checkpoint_fee(RuntimeOrigin::root(), chain_id, new_fee));

        assert_eq!(AvnAnchor::checkpoint_fee(chain_id), new_fee);
        System::assert_last_event(Event::CheckpointFeeUpdated { chain_id, new_fee }.into());
    });
}

#[test]
fn set_checkpoint_fee_fails_for_non_root() {
    new_test_ext().execute_with(|| {
        let non_root = create_account_id(1);
        let chain_id = 0;
        let new_fee = 100;

        assert_noop!(
            AvnAnchor::set_checkpoint_fee(RuntimeOrigin::signed(non_root), chain_id, new_fee),
            DispatchError::BadOrigin
        );
    });
}

#[test]
fn charge_fee_works() {
    new_test_ext().execute_with(|| {
        let handler = create_account_id(1);
        let chain_id = 0;
        let fee = 100;

        assert_ok!(AvnAnchor::set_checkpoint_fee(RuntimeOrigin::root(), chain_id, fee));

        assert_eq!(AvnAnchor::payment_nonces(handler.clone()), 0);

        assert_ok!(AvnAnchor::charge_fee(handler.clone(), chain_id));

        assert_eq!(AvnAnchor::payment_nonces(handler.clone()), 1);

        System::assert_last_event(
            Event::CheckpointFeeCharged { handler: handler.clone(), chain_id, fee, nonce: 0 }
                .into(),
        );
    });
}

#[test]
fn submit_checkpoint_charges_correct_fee() {
    new_test_ext().execute_with(|| {
        let handler = create_account_id(1);
        let chain_id = 0;
        let fee = 100;
        let name = bounded_vec(b"Test Chain");
        let checkpoint = H256::random();

        assert_ok!(AvnAnchor::register_chain_handler(RuntimeOrigin::signed(handler), name));
        assert_ok!(AvnAnchor::set_checkpoint_fee(RuntimeOrigin::root(), chain_id, fee));

        assert_ok!(AvnAnchor::submit_checkpoint_with_identity(
            RuntimeOrigin::signed(handler),
            checkpoint
        ));

        System::assert_has_event(
            Event::CheckpointSubmitted(handler, chain_id, 0, checkpoint).into(),
        );
        System::assert_has_event(
            Event::CheckpointFeeCharged { handler, chain_id, fee, nonce: 0 }.into(),
        );

        assert_eq!(AvnAnchor::payment_nonces(handler), 1);
    });
}

#[test]
fn different_chains_can_have_different_fees() {
    new_test_ext().execute_with(|| {
        let handler1 = create_account_id(1);
        let handler2 = create_account_id(2);
        let name1 = bounded_vec(b"Chain 1");
        let name2 = bounded_vec(b"Chain 2");
        let fee1 = 100;
        let fee2 = 200;

        assert_ok!(AvnAnchor::register_chain_handler(RuntimeOrigin::signed(handler1), name1));
        assert_ok!(AvnAnchor::register_chain_handler(RuntimeOrigin::signed(handler2), name2));

        let chain_id1 = AvnAnchor::chain_handlers(handler1).unwrap();
        let chain_id2 = AvnAnchor::chain_handlers(handler2).unwrap();

        assert_ok!(AvnAnchor::set_checkpoint_fee(RuntimeOrigin::root(), chain_id1, fee1));
        assert_ok!(AvnAnchor::set_checkpoint_fee(RuntimeOrigin::root(), chain_id2, fee2));

        assert_eq!(AvnAnchor::checkpoint_fee(chain_id1), fee1);
        assert_eq!(AvnAnchor::checkpoint_fee(chain_id2), fee2);
    });
}

#[test]
fn default_fee_applies_when_no_override() {
    new_test_ext().execute_with(|| {
        let handler = create_account_id(1);
        let name = bounded_vec(b"Test Chain");

        assert_ok!(AvnAnchor::register_chain_handler(RuntimeOrigin::signed(handler), name));

        let chain_id = AvnAnchor::chain_handlers(handler).unwrap();

        assert_eq!(AvnAnchor::get_checkpoint_fee(chain_id), DefaultCheckpointFee::get());
    });
}

#[test]
fn fee_override_works() {
    new_test_ext().execute_with(|| {
        let handler = create_account_id(1);
        let name = bounded_vec(b"Test Chain");
        let override_fee = 500u128;

        assert_ok!(AvnAnchor::register_chain_handler(RuntimeOrigin::signed(handler), name));

        let chain_id = AvnAnchor::chain_handlers(handler).unwrap();

        assert_eq!(AvnAnchor::get_checkpoint_fee(chain_id), DefaultCheckpointFee::get());

        assert_ok!(AvnAnchor::set_checkpoint_fee(RuntimeOrigin::root(), chain_id, override_fee));

        assert_eq!(AvnAnchor::get_checkpoint_fee(chain_id), override_fee);

        let other_chain_id = chain_id + 1;
        assert_eq!(AvnAnchor::get_checkpoint_fee(other_chain_id), DefaultCheckpointFee::get());
    });
}

#[test]
fn payment_nonces_increment_correctly() {
    new_test_ext().execute_with(|| {
        let handler = create_account_id(1);
        let name = bounded_vec(b"Test Chain");
        let checkpoint1 = H256::random();
        let checkpoint2 = H256::random();

        assert_ok!(AvnAnchor::register_chain_handler(RuntimeOrigin::signed(handler), name));

        let chain_id = AvnAnchor::chain_handlers(handler).unwrap();

        assert_ok!(AvnAnchor::set_checkpoint_fee(RuntimeOrigin::root(), chain_id, 100));

        assert_eq!(AvnAnchor::payment_nonces(handler), 0);

        assert_ok!(AvnAnchor::submit_checkpoint_with_identity(
            RuntimeOrigin::signed(handler),
            checkpoint1
        ));

        assert_eq!(AvnAnchor::payment_nonces(handler), 1);

        assert_ok!(AvnAnchor::submit_checkpoint_with_identity(
            RuntimeOrigin::signed(handler),
            checkpoint2
        ));

        assert_eq!(AvnAnchor::payment_nonces(handler), 2);
    });
}
