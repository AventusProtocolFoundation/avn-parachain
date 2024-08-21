use crate::{mock::*, ChainData, Error, Event};
use frame_support::{assert_noop, assert_ok, BoundedVec};
use sp_core::{ConstU32, H256};

fn bounded_vec(input: &[u8]) -> BoundedVec<u8, ConstU32<32>> {
    BoundedVec::<u8, ConstU32<32>>::try_from(input.to_vec()).unwrap()
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
fn update_chain_handler_fails_for_non_handler() {
    new_test_ext().execute_with(|| {
        let current_handler = 1;
        let new_handler = 2;
        let unauthorized_account = 3;
        let name = bounded_vec(b"Test Chain");

        assert_ok!(AvnAnchor::register_chain_handler(RuntimeOrigin::signed(current_handler), name.clone()));
        
        assert_noop!(
            AvnAnchor::update_chain_handler(RuntimeOrigin::signed(unauthorized_account), new_handler),
            Error::<TestRuntime>::ChainNotRegistered
        );

        // Verify that the handler hasn't changed
        let chain_data = AvnAnchor::chain_handlers(current_handler).unwrap();
        assert_eq!(chain_data.chain_id, 0);
        assert_eq!(chain_data.name, name);

        // Verify that the update is successful when initiated by the current handler
        assert_ok!(AvnAnchor::update_chain_handler(RuntimeOrigin::signed(current_handler), new_handler));

        // Verify that the handler has now changed
        assert!(AvnAnchor::chain_handlers(current_handler).is_none());
        let updated_chain_data = AvnAnchor::chain_handlers(new_handler).unwrap();
        assert_eq!(updated_chain_data.chain_id, 0);
        assert_eq!(updated_chain_data.name, name);

        System::assert_last_event(Event::ChainHandlerUpdated(current_handler, new_handler, 0, name).into());
    });
}

#[test]
fn register_multiple_chains_increments_chain_id() {
    new_test_ext().execute_with(|| {
        let handler1 = 1;
        let handler2 = 2;
        let name1 = bounded_vec(b"Chain 1");
        let name2 = bounded_vec(b"Chain 2");

        assert_ok!(AvnAnchor::register_chain_handler(RuntimeOrigin::signed(handler1), name1.clone()));
        assert_ok!(AvnAnchor::register_chain_handler(RuntimeOrigin::signed(handler2), name2.clone()));

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
