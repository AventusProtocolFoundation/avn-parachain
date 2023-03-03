// Copyright 2021 Aventus (UK) Ltd.
#![cfg(test)]

use crate::{mock::*, *};
use frame_support::{assert_noop, assert_ok};
use frame_system::RawOrigin;
use sp_avn_common::event_types::ValidEvents;
use sp_runtime::traits::BadOrigin;

mod test_set_ethereum_contract {
    use super::*;

    struct Context {
        origin: RuntimeOrigin,
        new_contract_address: H160,
        ethereum_contract: EthereumContracts,
    }

    impl Default for Context {
        fn default() -> Self {
            Context {
                origin: RawOrigin::Root.into(),
                new_contract_address: H160::from([15u8; 20]),
                ethereum_contract: EthereumContracts::ValidatorsManager,
            }
        }
    }

    impl Context {
        fn dispatch_set_ethereum_contract(&self) -> DispatchResult {
            return EthereumEvents::set_ethereum_contract(
                self.origin.clone(),
                self.ethereum_contract.clone(),
                self.new_contract_address.clone(),
            )
        }
    }

    mod successful_cases {
        use super::*;
        #[test]
        fn update_validators_manager_contract() {
            let mut ext = ExtBuilder::build_default().with_genesis_config().as_externality();
            ext.execute_with(|| {
                let context = Context::default();
                assert_ne!(
                    context.new_contract_address,
                    EthereumEvents::get_contract_address_for_non_nft_event(
                        &ValidEvents::AddedValidator
                    )
                    .unwrap()
                );

                assert_ok!(context.dispatch_set_ethereum_contract());
                assert_eq!(
                    context.new_contract_address,
                    EthereumEvents::get_contract_address_for_non_nft_event(
                        &ValidEvents::AddedValidator
                    )
                    .unwrap()
                );
            });
        }

        #[test]
        fn update_lifting_contract() {
            let mut ext = ExtBuilder::build_default().with_genesis_config().as_externality();
            ext.execute_with(|| {
                let context: Context =
                    Context { ethereum_contract: EthereumContracts::Lifting, ..Default::default() };
                assert_ne!(
                    context.new_contract_address,
                    EthereumEvents::get_contract_address_for_non_nft_event(&ValidEvents::Lifted)
                        .unwrap()
                );

                assert_ok!(context.dispatch_set_ethereum_contract());
                assert_eq!(
                    context.new_contract_address,
                    EthereumEvents::get_contract_address_for_non_nft_event(&ValidEvents::Lifted)
                        .unwrap()
                );
            });
        }

        #[test]
        fn update_nft_marketplace_contract_for_reserved_marketplace() {
            let mut ext = ExtBuilder::build_default().with_genesis_config().as_externality();
            ext.execute_with(|| {
                let mut context = Context::default();
                context.ethereum_contract = EthereumContracts::NftMarketplace;
                assert_eq!(
                    false,
                    NftT1Contracts::<TestRuntime>::contains_key(context.new_contract_address)
                );

                assert_ok!(context.dispatch_set_ethereum_contract());
                assert_eq!(
                    true,
                    NftT1Contracts::<TestRuntime>::contains_key(context.new_contract_address)
                );
            });
        }
    }

    mod fails_when {
        use super::*;

        #[test]
        fn zero_contract_should_fail() {
            let mut ext = ExtBuilder::build_default().with_genesis_config().as_externality();
            ext.execute_with(|| {
                let context: Context =
                    Context { new_contract_address: H160::zero(), ..Default::default() };

                assert_noop!(
                    context.dispatch_set_ethereum_contract(),
                    Error::<TestRuntime>::InvalidContractAddress
                );
                assert_ne!(
                    context.new_contract_address,
                    EthereumEvents::get_contract_address_for_non_nft_event(
                        &ValidEvents::AddedValidator
                    )
                    .unwrap()
                );
            });
        }

        #[test]
        fn origin_is_not_root() {
            let mut ext = ExtBuilder::build_default().with_genesis_config().as_externality();
            ext.execute_with(|| {
                let context: Context =
                    Context { origin: RuntimeOrigin::signed(account_id_0()), ..Default::default() };

                assert_noop!(context.dispatch_set_ethereum_contract(), BadOrigin);
                assert_ne!(
                    context.new_contract_address,
                    EthereumEvents::get_contract_address_for_non_nft_event(
                        &ValidEvents::AddedValidator
                    )
                    .unwrap()
                );
            });
        }

        #[test]
        fn origin_is_unsigned() {
            let mut ext = ExtBuilder::build_default().with_genesis_config().as_externality();
            ext.execute_with(|| {
                let context: Context =
                    Context { origin: RawOrigin::None.into(), ..Default::default() };

                assert_noop!(context.dispatch_set_ethereum_contract(), BadOrigin);
                assert_ne!(
                    context.new_contract_address,
                    EthereumEvents::get_contract_address_for_non_nft_event(
                        &ValidEvents::AddedValidator
                    )
                    .unwrap()
                );
            });
        }
    }
}
