// Copyright 2022 Aventus (UK) Ltd.
#![cfg(test)]

use crate::{mock::*, *};
use frame_support::{assert_noop, assert_ok};
use frame_system::RawOrigin;
use sp_runtime::traits::BadOrigin;

mod sanitise_nft_data_tests {
    use hex_literal::hex;

    use super::*;

    fn out_of_bounds_external_ref() -> WeakBoundedVec<u8, NftExternalRefBound> {
        let out_of_bounds_external_ref =
            vec![1u8; <NftExternalRefBound as sp_core::Get<u32>>::get() as usize + 1];

        WeakBoundedVec::<u8, NftExternalRefBound>::force_from(
            out_of_bounds_external_ref,
            Some("Expected to exceed bounds"),
        )
    }

    fn sanitised_external_ref() -> WeakBoundedVec<u8, NftExternalRefBound> {
        let out_of_bounds_external_ref =
            vec![1u8; <NftExternalRefBound as sp_core::Get<u32>>::get() as usize];

        WeakBoundedVec::<u8, NftExternalRefBound>::force_from(
            out_of_bounds_external_ref,
            Some("Expected to respect bounds"),
        )
    }

    struct Context {
        origin: RuntimeOrigin,
        t1_authority: H160,
        unique_id: NftUniqueId,
        info_id: NftInfoId,
        nft_owner: AccountId,
    }

    impl Default for Context {
        fn default() -> Self {
            let nft_owner = TestAccount::new([1u8; 32]);

            Context {
                origin: RawOrigin::Root.into(),
                nft_owner: nft_owner.account_id(),
                unique_id: NftManager::next_unique_id(),
                t1_authority: H160(hex!("11111AAAAA22222BBBBB11111AAAAA22222BBBBB")),
                info_id: NftManager::get_info_id_and_advance(),
            }
        }
    }

    impl Context {
        pub fn nft_id(&self) -> NftId {
            return NftManager::generate_nft_id_single_mint(&self.t1_authority, self.unique_id)
        }

        pub fn inject_nft_to_chain(&self) -> (Nft<AccountId>, NftInfo<AccountId>) {
            return NftManager::insert_single_nft_into_chain(
                self.info_id,
                Default::default(),
                self.t1_authority,
                self.nft_id(),
                out_of_bounds_external_ref(),
                self.nft_owner,
            )
        }

        fn dispatch_sanitise_nft_data(&self) -> DispatchResult {
            return NftManager::sanitise_nft_data(self.origin.clone(), vec![self.nft_id()])
        }
    }

    mod successful_cases {
        use super::*;
        #[test]
        fn update_publish_root_contract() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                let context = Context::default();
                context.inject_nft_to_chain();

                let nft_before_sanitisation =
                    NftManager::nfts(context.nft_id()).expect("Should exist as inserted");
                assert_eq!(
                    nft_before_sanitisation.unique_external_ref,
                    out_of_bounds_external_ref()
                );
                assert_eq!(true, NftManager::is_external_ref_used(out_of_bounds_external_ref()));
                assert_eq!(false, NftManager::is_external_ref_used(sanitised_external_ref()));

                assert_ok!(context.dispatch_sanitise_nft_data());

                let nft_after_sanitisation =
                    NftManager::nfts(context.nft_id()).expect("Should exist as inserted");
                assert_eq!(nft_after_sanitisation.unique_external_ref, sanitised_external_ref());
                assert_eq!(false, NftManager::is_external_ref_used(out_of_bounds_external_ref()));
                assert_eq!(true, NftManager::is_external_ref_used(sanitised_external_ref()));
            });
        }
    }

    mod fails_when {
        use sp_core::{sr25519, Pair};

        use super::*;

        #[test]
        fn origin_is_not_root() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                let account = sr25519::Pair::from_seed(&[1u8; 32]);
                let context: Context = Context {
                    origin: RuntimeOrigin::signed(account.public()),
                    ..Default::default()
                };
                assert_noop!(context.dispatch_sanitise_nft_data(), BadOrigin);
            });
        }

        #[test]
        fn origin_is_unsigned() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                let context: Context =
                    Context { origin: RawOrigin::None.into(), ..Default::default() };

                assert_noop!(context.dispatch_sanitise_nft_data(), BadOrigin);
            });
        }
    }
}
