use crate::{mock::*, ChainHandlers, Error, Event};
use frame_support::{assert_noop, assert_ok};
use sp_runtime::traits::BadOrigin;

#[test]
fn register_chain_handler_works() {
    new_test_ext().execute_with(|| {
        let chain_id = 1;
        let handler_account = 1;

        assert!(ChainHandlers::<TestRuntime>::get(chain_id).is_none());

        assert_ok!(AvnAnchor::register_chain_handler(
            RuntimeOrigin::signed(handler_account),
            chain_id,
            handler_account
        ));

        assert_eq!(ChainHandlers::<TestRuntime>::get(chain_id), Some(handler_account));

        System::assert_last_event(Event::ChainHandlerRegistered(chain_id, handler_account).into());
    });
}

#[test]
fn register_chain_handler_fails_for_unsigned() {
    new_test_ext().execute_with(|| {
        let chain_id = 1;
        let handler_account = 1;

        assert_noop!(
            AvnAnchor::register_chain_handler(RuntimeOrigin::none(), chain_id, handler_account),
            BadOrigin
        );
    });
}

#[test]
fn register_chain_handler_fails_for_existing_handler() {
    new_test_ext().execute_with(|| {
        let chain_id = 1;
        let handler_account = 1;

        assert_ok!(AvnAnchor::register_chain_handler(
            RuntimeOrigin::signed(handler_account),
            chain_id,
            handler_account
        ));

        assert_noop!(
            AvnAnchor::register_chain_handler(RuntimeOrigin::signed(2), chain_id, 2),
            Error::<TestRuntime>::HandlerAlreadyExists
        );
    });
}

#[test]
fn update_chain_handler_works() {
    new_test_ext().execute_with(|| {
        let chain_id = 1;
        let initial_handler = 1;
        let new_handler = 2;

        assert_ok!(AvnAnchor::register_chain_handler(
            RuntimeOrigin::signed(initial_handler),
            chain_id,
            initial_handler
        ));

        assert_ok!(AvnAnchor::update_chain_handler(
            RuntimeOrigin::signed(new_handler),
            chain_id,
            new_handler
        ));

        assert_eq!(ChainHandlers::<TestRuntime>::get(chain_id), Some(new_handler));

        System::assert_last_event(Event::ChainHandlerUpdated(chain_id, new_handler).into());
    });
}

#[test]
fn update_chain_handler_fails_for_unsigned() {
    new_test_ext().execute_with(|| {
        let chain_id = 1;
        let handler_account = 1;
        let new_handler = 2;

        assert_ok!(AvnAnchor::register_chain_handler(
            RuntimeOrigin::signed(handler_account),
            chain_id,
            handler_account
        ));

        assert_noop!(
            AvnAnchor::update_chain_handler(RuntimeOrigin::none(), chain_id, new_handler),
            BadOrigin
        );
    });
}

#[test]
fn update_chain_handler_fails_for_non_existent_handler() {
    new_test_ext().execute_with(|| {
        let chain_id = 1;
        let new_handler = 2;

        assert_noop!(
            AvnAnchor::update_chain_handler(
                RuntimeOrigin::signed(new_handler),
                chain_id,
                new_handler
            ),
            Error::<TestRuntime>::HandlerNotRegistered
        );
    });
}

#[test]
fn register_and_update_multiple_handlers() {
    new_test_ext().execute_with(|| {
        let chain_ids = vec![1, 2, 3];
        let initial_handlers = vec![10, 20, 30];
        let new_handlers = vec![15, 25, 35];

        for (&chain_id, &handler) in chain_ids.iter().zip(initial_handlers.iter()) {
            assert_ok!(AvnAnchor::register_chain_handler(
                RuntimeOrigin::signed(handler),
                chain_id,
                handler
            ));
            assert_eq!(ChainHandlers::<TestRuntime>::get(chain_id), Some(handler));
        }

        for (&chain_id, &new_handler) in chain_ids.iter().zip(new_handlers.iter()) {
            assert_ok!(AvnAnchor::update_chain_handler(
                RuntimeOrigin::signed(new_handler),
                chain_id,
                new_handler
            ));
            assert_eq!(ChainHandlers::<TestRuntime>::get(chain_id), Some(new_handler));
        }
    });
}
