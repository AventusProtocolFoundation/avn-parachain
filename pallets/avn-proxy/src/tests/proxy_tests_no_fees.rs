//Copyright 2021 Aventus Network Systems (UK) Ltd.

#![cfg(test)]
use crate::{mock::*, *};
use frame_support::{assert_noop, assert_ok, error::BadOrigin};
use frame_system::RawOrigin;
use pallet_nft_manager::Error as NftManagerError;
use sp_runtime::traits::Hash;

mod proxy_without_fees {
    use super::*;

    mod succeeds_when {
        use super::*;

        #[test]
        fn call_targets_a_whitelisted_pallet() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                let context: ProxyContext = Default::default();
                let inner_call = context.create_valid_inner_call();
                let call_hash = Hashing::hash_of(&inner_call);

                assert_eq!(false, proxy_event_emitted(context.relayer.account_id(), call_hash));
                assert_ok!(AvnProxy::proxy(
                    Origin::signed(context.relayer.account_id()),
                    inner_call,
                    None
                ));
                assert_eq!(true, proxy_event_emitted(context.relayer.account_id(), call_hash));
            })
        }

        #[test]
        fn call_is_proxied_with_good_parameters() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                let context: ProxyContext = Default::default();
                let inner_call = create_signed_mint_single_nft_call(&context);

                assert_eq!(false, single_nft_minted_events_emitted());
                assert_ok!(AvnProxy::proxy(
                    Origin::signed(context.relayer.account_id()),
                    inner_call,
                    None
                ));
                assert_eq!(true, single_nft_minted_events_emitted());
            })
        }
    }

    mod fails_when {
        use super::*;

        #[test]
        fn origin_is_unsigned() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                let context: ProxyContext = Default::default();
                let inner_call = context.create_valid_inner_call();

                let unsigned_origin = RawOrigin::None.into();

                assert_noop!(AvnProxy::proxy(unsigned_origin, inner_call, None), BadOrigin);
            });
        }

        #[test]
        fn call_is_not_allowed_to_be_proxied() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                let context: ProxyContext = Default::default();

                let invalid_inner_call = context.create_invalid_inner_call();

                assert_noop!(
                    AvnProxy::proxy(
                        Origin::signed(context.relayer.account_id()),
                        invalid_inner_call,
                        None
                    ),
                    Error::<TestRuntime>::TransactionNotSupported
                );

                assert_eq!(System::events().len(), 0);
            })
        }

        #[test]
        fn sender_has_not_signed_relayer() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                let context: ProxyContext = Default::default();
                let inner_call = context.create_valid_inner_call();
                let call_hash = Hashing::hash_of(&inner_call);

                let invalid_relayer = context.signer.account_id();

                assert_eq!(false, proxy_event_emitted(context.relayer.account_id(), call_hash));
                assert_noop!(
                    AvnProxy::proxy(Origin::signed(invalid_relayer), inner_call, None),
                    Error::<TestRuntime>::UnauthorizedProxyTransaction
                );
            })
        }

        #[test]
        fn call_is_proxying_the_proxy_extrinsic() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                let context: ProxyContext = Default::default();

                let invalid_inner_call = context.create_proxy_call();

                assert_noop!(
                    AvnProxy::proxy(
                        Origin::signed(context.relayer.account_id()),
                        invalid_inner_call,
                        None
                    ),
                    Error::<TestRuntime>::TransactionNotSupported
                );

                assert_eq!(System::events().len(), 0);
            })
        }

        #[test]
        fn inner_call_fails_to_execute() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                let context: ProxyContext = Default::default();
                let inner_call = create_signed_mint_single_nft_call(&context);

                assert_eq!(false, single_nft_minted_events_emitted());
                assert_ok!(AvnProxy::proxy(
                    Origin::signed(context.relayer.account_id()),
                    inner_call,
                    None
                ));
                assert_eq!(single_nft_minted_events_count(), 1);

                let inner_call_with_duplicate_external_ref =
                    create_signed_mint_single_nft_call(&context);

                assert_noop!(
                    AvnProxy::proxy(
                        Origin::signed(context.relayer.account_id()),
                        inner_call_with_duplicate_external_ref,
                        None
                    ),
                    NftManagerError::<TestRuntime>::ExternalRefIsAlreadyInUse
                );
            })
        }
    }
}
