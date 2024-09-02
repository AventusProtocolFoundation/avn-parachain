use crate::{mock::*, ChainData, Error, Event};
use frame_support::{assert_noop, assert_ok, BoundedVec};
use sp_avn_common::Proof;
use sp_core::{Pair, sr25519, ConstU32, H256};

fn bounded_vec(input: &[u8]) -> BoundedVec<u8, ConstU32<32>> {
    BoundedVec::<u8, ConstU32<32>>::try_from(input.to_vec()).unwrap()
}

fn create_proof(signer: u64, relayer: u64, payload: &[u8]) -> Proof<sr25519::Signature, u64> {
    let pair = sr25519::Pair::from_seed(&[signer as u8; 32]);
    let signature = pair.sign(payload);
    Proof { signature, relayer, signer }
}

#[test]
fn register_chain_handler_works() {
    new_test_ext().execute_with(|| {
        let handler = 1;
        let name = bounded_vec(b"Test Chain");

        assert_ok!(AvnAnchor::register_chain_handler(RuntimeOrigin::signed(handler), name.clone()));

        let chain_data = AvnAnchor::chain_handlers(handler).unwrap();
        assert_eq!(chain_data.chain_id, 0);
        assert_eq!(chain_data.name, name);

        System::assert_last_event(Event::ChainHandlerRegistered(handler, 0, name).into());
    });
}

#[test]
fn register_chain_handler_fails_for_existing_handler() {
    new_test_ext().execute_with(|| {
        let handler = 1;
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
        let handler = 1;
        let empty_name = bounded_vec(b"");

        assert_noop!(
            AvnAnchor::register_chain_handler(RuntimeOrigin::signed(handler), empty_name),
            Error::<TestRuntime>::EmptyChainName
        );
    });
}

#[test]
fn update_chain_handler_works() {
    new_test_ext().execute_with(|| {
        let old_handler = 1;
        let new_handler = 2;
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
        let chain_data = AvnAnchor::chain_handlers(new_handler).unwrap();
        assert_eq!(chain_data.chain_id, 0);
        assert_eq!(chain_data.name, name);

        System::assert_last_event(
            Event::ChainHandlerUpdated(old_handler, new_handler, 0, name).into(),
        );
    });
}

#[test]
fn update_chain_handler_fails_for_non_existent_handler() {
    new_test_ext().execute_with(|| {
        let old_handler = 1;
        let new_handler = 2;

        assert_noop!(
            AvnAnchor::update_chain_handler(RuntimeOrigin::signed(old_handler), new_handler),
            Error::<TestRuntime>::ChainNotRegistered
        );
    });
}

#[test]
fn update_chain_handler_fails_for_already_registered_new_handler() {
    new_test_ext().execute_with(|| {
        let handler1 = 1;
        let handler2 = 2;

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
        let current_handler = 1;
        let new_handler = 2;
        let unauthorized_account = 3;
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

        // Verify that the handler hasn't changed
        let chain_data = AvnAnchor::chain_handlers(current_handler).unwrap();
        assert_eq!(chain_data.chain_id, 0);
        assert_eq!(chain_data.name, name);

        // Verify that the update is successful when initiated by the current handler
        assert_ok!(AvnAnchor::update_chain_handler(
            RuntimeOrigin::signed(current_handler),
            new_handler
        ));

        // Verify that the handler has now changed
        assert!(AvnAnchor::chain_handlers(current_handler).is_none());
        let updated_chain_data = AvnAnchor::chain_handlers(new_handler).unwrap();
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
        let handler = 1;
        let name = bounded_vec(b"Test Chain");
        let checkpoint = H256::random();

        assert_ok!(AvnAnchor::register_chain_handler(RuntimeOrigin::signed(handler), name));
        assert_ok!(AvnAnchor::submit_checkpoint_with_identity(
            RuntimeOrigin::signed(handler),
            checkpoint
        ));

        assert_eq!(AvnAnchor::checkpoints(0, 0), checkpoint);

        System::assert_last_event(Event::CheckpointSubmitted(handler, 0, 0, checkpoint).into());
    });
}

#[test]
fn submit_checkpoint_with_identity_fails_for_unregistered_handler() {
    new_test_ext().execute_with(|| {
        let handler = 1;
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
        let handler = 1;
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

        System::assert_has_event(Event::CheckpointSubmitted(handler, 0, 0, checkpoint1).into());
        System::assert_has_event(Event::CheckpointSubmitted(handler, 0, 1, checkpoint2).into());
    });
}

#[test]
fn register_multiple_chains_increments_chain_id() {
    new_test_ext().execute_with(|| {
        let handler1 = 1;
        let handler2 = 2;
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

        let chain_data1 = AvnAnchor::chain_handlers(handler1).unwrap();
        let chain_data2 = AvnAnchor::chain_handlers(handler2).unwrap();

        assert_eq!(chain_data1.chain_id, 0);
        assert_eq!(chain_data1.name, name1);
        assert_eq!(chain_data2.chain_id, 1);
        assert_eq!(chain_data2.name, name2);

        System::assert_has_event(Event::ChainHandlerRegistered(handler1, 0, name1).into());
        System::assert_has_event(Event::ChainHandlerRegistered(handler2, 1, name2).into());
    });
}

#[test]
fn signed_register_chain_handler_works() {
    new_test_ext().execute_with(|| {
        let handler = 1u64;
        let relayer = 2u64;
        let name = bounded_vec(b"Test Chain");
        let nonce = AvnAnchor::nonces(&handler);
        let payload = (b"register_chain_handler", handler, &handler, &name, nonce).encode();
        let proof = create_proof(handler, relayer, &payload);

        assert_ok!(AvnAnchor::signed_register_chain_handler(
            RuntimeOrigin::signed(handler),
            proof,
            handler,
            name.clone()
        ));

        let chain_data = AvnAnchor::chain_handlers(handler).unwrap();
        assert_eq!(chain_data.chain_id, 0);
        assert_eq!(chain_data.name, name);

        System::assert_last_event(Event::ChainHandlerRegistered(handler, 0, name).into());
        assert_eq!(AvnAnchor::nonces(&handler), 1);
    });
}

#[test]
fn signed_update_chain_handler_works() {
    new_test_ext().execute_with(|| {
        let old_handler = 1u64;
        let new_handler = 2u64;
        let relayer = 3u64;
        let name = bounded_vec(b"Test Chain");

        assert_ok!(AvnAnchor::register_chain_handler(
            RuntimeOrigin::signed(old_handler),
            name.clone()
        ));

        let nonce = AvnAnchor::nonces(&old_handler);
        let payload = (b"update_chain_handler", old_handler, &old_handler, &new_handler, nonce).encode();
        let proof = create_proof(old_handler, relayer, &payload);

        assert_ok!(AvnAnchor::signed_update_chain_handler(
            RuntimeOrigin::signed(old_handler),
            proof,
            old_handler,
            new_handler
        ));

        assert!(AvnAnchor::chain_handlers(old_handler).is_none());
        let chain_data = AvnAnchor::chain_handlers(new_handler).unwrap();
        assert_eq!(chain_data.chain_id, 0);
        assert_eq!(chain_data.name, name);

        System::assert_last_event(
            Event::ChainHandlerUpdated(old_handler, new_handler, 0, name).into(),
        );
        assert_eq!(AvnAnchor::nonces(&old_handler), 1);
    });
}

#[test]
fn signed_submit_checkpoint_with_identity_works() {
    new_test_ext().execute_with(|| {
        let handler = 1u64;
        let relayer = 2u64;
        let name = bounded_vec(b"Test Chain");
        let checkpoint = H256::random();

        assert_ok!(AvnAnchor::register_chain_handler(RuntimeOrigin::signed(handler), name));

        let nonce = AvnAnchor::nonces(&handler);
        let payload = (b"submit_checkpoint", handler, &handler, &checkpoint, nonce).encode();
        let proof = create_proof(handler, relayer, &payload);

        assert_ok!(AvnAnchor::signed_submit_checkpoint_with_identity(
            RuntimeOrigin::signed(handler),
            proof,
            handler,
            checkpoint
        ));

        assert_eq!(AvnAnchor::checkpoints(0, 0), checkpoint);

        System::assert_last_event(Event::CheckpointSubmitted(handler, 0, 0, checkpoint).into());
        assert_eq!(AvnAnchor::nonces(&handler), 1);
    });
}