// This file is part of Aventus.
// Copyright (C) 2022 Aventus Network Services (UK) Ltd.

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

#![cfg(test)]
use super::*;
use crate::{
    mock::{AccountId, RuntimeCall as MockCall, RuntimeOrigin as Origin, *},
    Call,
};
use codec::Encode;
use frame_support::{assert_noop, assert_ok, error::BadOrigin};
use frame_system::RawOrigin;
use mock::RuntimeEvent as Event;
use sp_core::{sr25519::Pair, H256, U256};

fn build_proof(
    signer: &AccountId,
    relayer: &AccountId,
    signature: Signature,
) -> Proof<Signature, AccountId> {
    return Proof { signer: *signer, relayer: *relayer, signature }
}

struct Context {
    nft_owner_account: AccountId,
    nft_owner_key_pair: Pair,
    relayer: AccountId,
    nft_id: U256,
    new_nft_owner_account: AccountId,
    t2_transfer_to_public_key: H256,
    op_id: u64,
}

impl Default for Context {
    fn default() -> Self {
        let nft_id = U256::from([
            144, 32, 76, 127, 69, 26, 191, 42, 121, 72, 235, 94, 179, 147, 69, 29, 167, 189, 8, 44,
            104, 83, 241, 253, 146, 114, 166, 195, 200, 254, 120, 78,
        ]);
        let nft_owner = TestAccount::new([1u8; 32]);
        let relayer = TestAccount::new([2u8; 32]);
        let t2_transfer_to_public_key = H256::from([1; 32]);
        let new_nft_owner_account = AccountId::decode(&mut t2_transfer_to_public_key.as_bytes())
            .expect("32 bytes will always decode into an AccountId");
        let op_id = 0;

        Context {
            nft_owner_account: nft_owner.account_id(),
            nft_owner_key_pair: nft_owner.key_pair(),
            relayer: relayer.account_id(),
            nft_id,
            new_nft_owner_account,
            t2_transfer_to_public_key,
            op_id,
        }
    }
}

impl Context {
    fn setup(&self) {
        let nft = Nft::new(
            self.nft_id,
            NftManager::get_info_id_and_advance(),
            WeakBoundedVec::try_from(String::from("Offchain location of NFT").into_bytes())
                .expect("Unique external reference bound was exceeded."),
            self.nft_owner_account,
        );
        <NftManager as Store>::Nfts::insert(self.nft_id, &nft);
        <NftManager as Store>::NftOpenForSale::insert(&self.nft_id, NftSaleType::Fiat);
    }

    fn create_signed_transfer_fiat_nft_call(&self) -> Box<<TestRuntime as Config>::RuntimeCall> {
        let proof = self.create_signed_transfer_fiat_nft_proof();

        return Box::new(MockCall::NftManager(
            super::Call::<TestRuntime>::signed_transfer_fiat_nft {
                proof,
                nft_id: self.nft_id,
                t2_transfer_to_public_key: self.t2_transfer_to_public_key,
            },
        ))
    }

    fn create_signed_transfer_fiat_nft_proof(&self) -> Proof<Signature, AccountId> {
        return create_proof_for_signed_transfer_fiat_nft(
            &self.relayer,
            &self.nft_owner_account,
            &self.nft_owner_key_pair,
            &self.nft_id,
            &self.t2_transfer_to_public_key,
            &self.op_id,
        )
    }

    fn signed_transfer_fiat_nft_event_emitted(&self) -> bool {
        return System::events().iter().any(|a| {
            a.event ==
                Event::NftManager(crate::Event::<TestRuntime>::FiatNftTransfer {
                    nft_id: self.nft_id,
                    sender: self.nft_owner_account,
                    new_owner: self.new_nft_owner_account,
                    sale_type: NftSaleType::Fiat,
                    op_id: self.op_id,
                })
        })
    }

    fn call_dispatched_event_emitted(
        &self,
        call: &Box<<TestRuntime as Config>::RuntimeCall>,
    ) -> bool {
        let relayer = TestAccount::new([2u8; 32]);
        return System::events().iter().any(|a| {
            a.event ==
                Event::NftManager(crate::Event::<TestRuntime>::CallDispatched {
                    relayer: relayer.account_id(),
                    hash: Hashing::hash_of(call),
                })
        })
    }
}

mod proxy_signed_transfer_fiat_nft {
    use super::*;

    mod succeeds_implies_that {
        use super::*;

        #[test]
        fn nft_is_not_listed_for_sale_anymore() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                let context = Context::default();
                context.setup();
                let call = context.create_signed_transfer_fiat_nft_call();

                assert_eq!(true, <NftOpenForSale<TestRuntime>>::contains_key(&context.nft_id));

                assert_ok!(NftManager::proxy(Origin::signed(context.relayer), call));

                assert_eq!(false, <NftOpenForSale<TestRuntime>>::contains_key(&context.nft_id));
            });
        }

        #[test]
        fn nft_nonce_is_increased() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                let context = Context::default();
                context.setup();
                let call = context.create_signed_transfer_fiat_nft_call();

                let original_nonce = NftManager::nfts(context.nft_id).unwrap().nonce;

                assert_ok!(NftManager::proxy(Origin::signed(context.relayer), call));

                assert_eq!(original_nonce + 1u64, NftManager::nfts(context.nft_id).unwrap().nonce);
            });
        }

        #[test]
        fn signed_transfer_fiat_nft_event_emitted() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                let context = Context::default();
                context.setup();
                let call = context.create_signed_transfer_fiat_nft_call();

                assert_eq!(false, context.signed_transfer_fiat_nft_event_emitted());

                assert_ok!(NftManager::proxy(Origin::signed(context.relayer), call));

                assert_eq!(true, context.signed_transfer_fiat_nft_event_emitted());
            });
        }

        #[test]
        fn call_dispatched_event_emitted() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                let context = Context::default();
                context.setup();
                let call = context.create_signed_transfer_fiat_nft_call();

                assert_eq!(false, context.call_dispatched_event_emitted(&call));

                assert_ok!(NftManager::proxy(Origin::signed(context.relayer), call.clone()));

                assert_eq!(true, context.call_dispatched_event_emitted(&call));
            });
        }
    }

    mod fails_when {
        use super::*;

        #[test]
        fn origin_is_unsigned() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                let context = Context::default();
                context.setup();
                let call = context.create_signed_transfer_fiat_nft_call();

                assert_noop!(NftManager::proxy(RawOrigin::None.into(), call.clone()), BadOrigin);
            });
        }

        #[test]
        fn nft_is_not_listed_for_sale() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                let context = Context::default();
                context.setup();
                let call = context.create_signed_transfer_fiat_nft_call();

                <NftManager as Store>::NftOpenForSale::remove(&context.nft_id);

                assert_noop!(
                    NftManager::proxy(Origin::signed(context.relayer), call.clone()),
                    Error::<TestRuntime>::NftNotListedForSale
                );
            });
        }

        #[test]
        fn nft_is_not_listed_for_fiat_sale() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                let context = Context::default();
                context.setup();
                let call = context.create_signed_transfer_fiat_nft_call();

                <NftManager as Store>::NftOpenForSale::mutate(context.nft_id, |nft_sale_type| {
                    *nft_sale_type = NftSaleType::Ethereum;
                });

                assert_noop!(
                    NftManager::proxy(Origin::signed(context.relayer), call.clone()),
                    Error::<TestRuntime>::NftNotListedForFiatSale
                );
            });
        }

        #[test]
        fn sender_is_not_owner() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                let context = Context::default();
                context.setup();
                let call = context.create_signed_transfer_fiat_nft_call();

                let other_owner = TestAccount::new([5u8; 32]);
                <NftManager as Store>::Nfts::mutate(context.nft_id, |maybe_nft| {
                    maybe_nft.as_mut().map(|nft| nft.owner = other_owner.account_id())
                });

                assert_noop!(
                    NftManager::proxy(Origin::signed(context.relayer), call.clone()),
                    Error::<TestRuntime>::SenderIsNotOwner
                );
            });
        }

        #[test]
        fn nft_is_locked() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                let context = Context::default();
                context.setup();
                let call = context.create_signed_transfer_fiat_nft_call();

                <NftManager as Store>::Nfts::mutate(context.nft_id, |maybe_nft| {
                    maybe_nft.as_mut().map(|nft| nft.is_locked = true)
                });

                assert_noop!(
                    NftManager::proxy(Origin::signed(context.relayer), call.clone()),
                    Error::<TestRuntime>::NftIsLocked
                );
            });
        }

        #[test]
        fn mismatched_proof_context() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                let context = Context::default();
                context.setup();

                let other_context: &'static [u8] = b"authorization for something else";
                let data_to_sign = (
                    other_context,
                    &context.relayer,
                    &context.nft_id,
                    &context.t2_transfer_to_public_key,
                    context.op_id,
                );
                let signature = sign(&context.nft_owner_key_pair, &data_to_sign.encode());
                let proof = build_proof(&context.nft_owner_account, &context.relayer, signature);
                let call = Box::new(MockCall::NftManager(
                    super::Call::<TestRuntime>::signed_transfer_fiat_nft {
                        proof,
                        nft_id: context.nft_id,
                        t2_transfer_to_public_key: context.t2_transfer_to_public_key,
                    },
                ));

                assert_noop!(
                    NftManager::proxy(Origin::signed(context.relayer), call.clone()),
                    Error::<TestRuntime>::UnauthorizedSignedTransferFiatNftTransaction
                );
            });
        }

        #[test]
        fn mismatched_proof_other_nft_owner_key_pair() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                let context = Context::default();
                context.setup();
                let other_nft_owner_account = TestAccount::new([5u8; 32]);
                let proof = create_proof_for_signed_transfer_fiat_nft(
                    &context.relayer,
                    &context.nft_owner_account,
                    &other_nft_owner_account.key_pair(),
                    &context.nft_id,
                    &context.t2_transfer_to_public_key,
                    &context.op_id,
                );
                let call = Box::new(MockCall::NftManager(
                    super::Call::<TestRuntime>::signed_transfer_fiat_nft {
                        proof,
                        nft_id: context.nft_id,
                        t2_transfer_to_public_key: context.t2_transfer_to_public_key,
                    },
                ));

                assert_noop!(
                    NftManager::proxy(Origin::signed(context.relayer), call.clone()),
                    Error::<TestRuntime>::UnauthorizedSignedTransferFiatNftTransaction
                );
            });
        }

        #[test]
        fn mismatched_proof_other_nft_id() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                let context = Context::default();
                context.setup();

                let other_nft_id = U256::from([1u8; 32]);
                let proof = create_proof_for_signed_transfer_fiat_nft(
                    &context.relayer,
                    &context.nft_owner_account,
                    &context.nft_owner_key_pair,
                    &other_nft_id,
                    &context.t2_transfer_to_public_key,
                    &context.op_id,
                );
                let call = Box::new(MockCall::NftManager(
                    super::Call::<TestRuntime>::signed_transfer_fiat_nft {
                        proof,
                        nft_id: context.nft_id,
                        t2_transfer_to_public_key: context.t2_transfer_to_public_key,
                    },
                ));

                assert_noop!(
                    NftManager::proxy(Origin::signed(context.relayer), call.clone()),
                    Error::<TestRuntime>::UnauthorizedSignedTransferFiatNftTransaction
                );
            });
        }

        #[test]
        fn mismatched_proof_other_t2_transfer_to_public_key() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                let context = Context::default();
                context.setup();

                let other_t2_transfer_to_public_key = H256::from([9; 32]);
                let proof = create_proof_for_signed_transfer_fiat_nft(
                    &context.relayer,
                    &context.nft_owner_account,
                    &context.nft_owner_key_pair,
                    &context.nft_id,
                    &other_t2_transfer_to_public_key,
                    &context.op_id,
                );
                let call = Box::new(MockCall::NftManager(
                    super::Call::<TestRuntime>::signed_transfer_fiat_nft {
                        proof,
                        nft_id: context.nft_id,
                        t2_transfer_to_public_key: context.t2_transfer_to_public_key,
                    },
                ));

                assert_noop!(
                    NftManager::proxy(Origin::signed(context.relayer), call.clone()),
                    Error::<TestRuntime>::UnauthorizedSignedTransferFiatNftTransaction
                );
            });
        }

        #[test]
        fn mismatched_proof_other_op_id() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                let context = Context::default();
                context.setup();

                let other_op_id = 111;
                let proof = create_proof_for_signed_transfer_fiat_nft(
                    &context.relayer,
                    &context.nft_owner_account,
                    &context.nft_owner_key_pair,
                    &context.nft_id,
                    &context.t2_transfer_to_public_key,
                    &other_op_id,
                );
                let call = Box::new(MockCall::NftManager(
                    super::Call::<TestRuntime>::signed_transfer_fiat_nft {
                        proof,
                        nft_id: context.nft_id,
                        t2_transfer_to_public_key: context.t2_transfer_to_public_key,
                    },
                ));

                assert_noop!(
                    NftManager::proxy(Origin::signed(context.relayer), call.clone()),
                    Error::<TestRuntime>::UnauthorizedSignedTransferFiatNftTransaction
                );
            });
        }
    }
}

mod signed_transfer_fiat_nft {
    use super::*;

    mod succeeds_implies_that {
        use super::*;

        #[test]
        fn nft_is_not_listed_for_sale_anymore() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                let context = Context::default();
                context.setup();
                let proof = context.create_signed_transfer_fiat_nft_proof();

                assert_eq!(true, <NftOpenForSale<TestRuntime>>::contains_key(&context.nft_id));

                assert_ok!(NftManager::signed_transfer_fiat_nft(
                    Origin::signed(context.nft_owner_account),
                    proof,
                    context.nft_id,
                    context.t2_transfer_to_public_key
                ));

                assert_eq!(false, <NftOpenForSale<TestRuntime>>::contains_key(&context.nft_id));
            });
        }

        #[test]
        fn nft_nonce_is_increased() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                let context = Context::default();
                context.setup();
                let proof = context.create_signed_transfer_fiat_nft_proof();

                let original_nonce = NftManager::nfts(context.nft_id).unwrap().nonce;

                assert_ok!(NftManager::signed_transfer_fiat_nft(
                    Origin::signed(context.nft_owner_account),
                    proof,
                    context.nft_id,
                    context.t2_transfer_to_public_key
                ));

                assert_eq!(original_nonce + 1u64, NftManager::nfts(context.nft_id).unwrap().nonce);
            });
        }

        #[test]
        fn signed_transfer_fiat_nft_event_emitted() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                let context = Context::default();
                context.setup();
                let proof = context.create_signed_transfer_fiat_nft_proof();

                assert_eq!(false, context.signed_transfer_fiat_nft_event_emitted());

                assert_ok!(NftManager::signed_transfer_fiat_nft(
                    Origin::signed(context.nft_owner_account),
                    proof,
                    context.nft_id,
                    context.t2_transfer_to_public_key
                ));

                assert_eq!(true, context.signed_transfer_fiat_nft_event_emitted());
            });
        }
    }

    mod fails_when {
        use super::*;

        #[test]
        fn origin_is_unsigned() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                let context = Context::default();
                context.setup();
                let proof = context.create_signed_transfer_fiat_nft_proof();

                assert_noop!(
                    NftManager::signed_transfer_fiat_nft(
                        RawOrigin::None.into(),
                        proof,
                        context.nft_id,
                        context.t2_transfer_to_public_key
                    ),
                    BadOrigin
                );
            });
        }

        #[test]
        fn sender_is_not_signer() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                let context = Context::default();
                context.setup();
                let proof = context.create_signed_transfer_fiat_nft_proof();

                let other_sender = TestAccount::new([5u8; 32]);

                assert_noop!(
                    NftManager::signed_transfer_fiat_nft(
                        Origin::signed(other_sender.account_id()),
                        proof,
                        context.nft_id,
                        context.t2_transfer_to_public_key
                    ),
                    Error::<TestRuntime>::SenderIsNotSigner
                );
            });
        }

        #[test]
        fn nft_is_not_listed_for_sale() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                let context = Context::default();
                context.setup();
                let proof = context.create_signed_transfer_fiat_nft_proof();

                <NftManager as Store>::NftOpenForSale::remove(&context.nft_id);

                assert_noop!(
                    NftManager::signed_transfer_fiat_nft(
                        Origin::signed(context.nft_owner_account),
                        proof,
                        context.nft_id,
                        context.t2_transfer_to_public_key
                    ),
                    Error::<TestRuntime>::NftNotListedForSale
                );
            });
        }

        #[test]
        fn nft_is_not_listed_for_fiat_sale() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                let context = Context::default();
                context.setup();
                let proof = context.create_signed_transfer_fiat_nft_proof();

                <NftManager as Store>::NftOpenForSale::mutate(context.nft_id, |nft_sale_type| {
                    *nft_sale_type = NftSaleType::Ethereum;
                });

                assert_noop!(
                    NftManager::signed_transfer_fiat_nft(
                        Origin::signed(context.nft_owner_account),
                        proof,
                        context.nft_id,
                        context.t2_transfer_to_public_key
                    ),
                    Error::<TestRuntime>::NftNotListedForFiatSale
                );
            });
        }

        #[test]
        fn sender_is_not_owner() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                let context = Context::default();
                context.setup();
                let proof = context.create_signed_transfer_fiat_nft_proof();

                let other_owner = TestAccount::new([5u8; 32]);
                <NftManager as Store>::Nfts::mutate(context.nft_id, |maybe_nft| {
                    maybe_nft.as_mut().map(|nft| nft.owner = other_owner.account_id())
                });

                assert_noop!(
                    NftManager::signed_transfer_fiat_nft(
                        Origin::signed(context.nft_owner_account),
                        proof,
                        context.nft_id,
                        context.t2_transfer_to_public_key
                    ),
                    Error::<TestRuntime>::SenderIsNotOwner
                );
            });
        }

        #[test]
        fn nft_is_locked() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                let context = Context::default();
                context.setup();
                let proof = context.create_signed_transfer_fiat_nft_proof();

                <NftManager as Store>::Nfts::mutate(context.nft_id, |maybe_nft| {
                    maybe_nft.as_mut().map(|nft| nft.is_locked = true)
                });

                assert_noop!(
                    NftManager::signed_transfer_fiat_nft(
                        Origin::signed(context.nft_owner_account),
                        proof,
                        context.nft_id,
                        context.t2_transfer_to_public_key
                    ),
                    Error::<TestRuntime>::NftIsLocked
                );
            });
        }

        #[test]
        fn mismatched_proof_context() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                let context = Context::default();
                context.setup();

                let other_context: &'static [u8] = b"authorization for something else";
                let data_to_sign = (
                    other_context,
                    &context.relayer,
                    &context.nft_id,
                    &context.t2_transfer_to_public_key,
                    context.op_id,
                );
                let signature = sign(&context.nft_owner_key_pair, &data_to_sign.encode());
                let proof = build_proof(&context.nft_owner_account, &context.relayer, signature);

                assert_noop!(
                    NftManager::signed_transfer_fiat_nft(
                        Origin::signed(context.nft_owner_account),
                        proof,
                        context.nft_id,
                        context.t2_transfer_to_public_key
                    ),
                    Error::<TestRuntime>::UnauthorizedSignedTransferFiatNftTransaction
                );
            });
        }

        #[test]
        fn mismatched_proof_other_nft_owner_key_pair() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                let context = Context::default();
                context.setup();
                let other_nft_owner_account = TestAccount::new([5u8; 32]);
                let proof = create_proof_for_signed_transfer_fiat_nft(
                    &context.relayer,
                    &context.nft_owner_account,
                    &other_nft_owner_account.key_pair(),
                    &context.nft_id,
                    &context.t2_transfer_to_public_key,
                    &context.op_id,
                );

                assert_noop!(
                    NftManager::signed_transfer_fiat_nft(
                        Origin::signed(context.nft_owner_account),
                        proof,
                        context.nft_id,
                        context.t2_transfer_to_public_key
                    ),
                    Error::<TestRuntime>::UnauthorizedSignedTransferFiatNftTransaction
                );
            });
        }

        #[test]
        fn mismatched_proof_other_nft_id() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                let context = Context::default();
                context.setup();

                let other_nft_id = U256::from([1u8; 32]);
                let proof = create_proof_for_signed_transfer_fiat_nft(
                    &context.relayer,
                    &context.nft_owner_account,
                    &context.nft_owner_key_pair,
                    &other_nft_id,
                    &context.t2_transfer_to_public_key,
                    &context.op_id,
                );

                assert_noop!(
                    NftManager::signed_transfer_fiat_nft(
                        Origin::signed(context.nft_owner_account),
                        proof,
                        context.nft_id,
                        context.t2_transfer_to_public_key
                    ),
                    Error::<TestRuntime>::UnauthorizedSignedTransferFiatNftTransaction
                );
            });
        }

        #[test]
        fn mismatched_proof_other_t2_transfer_to_public_key() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                let context = Context::default();
                context.setup();

                let other_t2_transfer_to_public_key = H256::from([9; 32]);
                let proof = create_proof_for_signed_transfer_fiat_nft(
                    &context.relayer,
                    &context.nft_owner_account,
                    &context.nft_owner_key_pair,
                    &context.nft_id,
                    &other_t2_transfer_to_public_key,
                    &context.op_id,
                );

                assert_noop!(
                    NftManager::signed_transfer_fiat_nft(
                        Origin::signed(context.nft_owner_account),
                        proof,
                        context.nft_id,
                        context.t2_transfer_to_public_key
                    ),
                    Error::<TestRuntime>::UnauthorizedSignedTransferFiatNftTransaction
                );
            });
        }

        #[test]
        fn mismatched_proof_other_op_id() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                let context = Context::default();
                context.setup();

                let other_op_id = 111;
                let proof = create_proof_for_signed_transfer_fiat_nft(
                    &context.relayer,
                    &context.nft_owner_account,
                    &context.nft_owner_key_pair,
                    &context.nft_id,
                    &context.t2_transfer_to_public_key,
                    &other_op_id,
                );

                assert_noop!(
                    NftManager::signed_transfer_fiat_nft(
                        Origin::signed(context.nft_owner_account),
                        proof,
                        context.nft_id,
                        context.t2_transfer_to_public_key
                    ),
                    Error::<TestRuntime>::UnauthorizedSignedTransferFiatNftTransaction
                );
            });
        }

        #[test]
        fn transfer_to_is_empty() {
            let mut ext = ExtBuilder::build_default().as_externality();
            ext.execute_with(|| {
                let mut context = Context::default();
                context.t2_transfer_to_public_key = H256::zero();
                context.setup();
                let proof = context.create_signed_transfer_fiat_nft_proof();

                assert_noop!(
                    NftManager::signed_transfer_fiat_nft(
                        Origin::signed(context.nft_owner_account),
                        proof,
                        context.nft_id,
                        context.t2_transfer_to_public_key
                    ),
                    Error::<TestRuntime>::TransferToIsMandatory
                );
            });
        }
    }
}

mod get_proof {
    use super::*;

    #[test]
    fn succeeds_for_valid_signed_transfer_fiat_nft_call() {
        let mut ext = ExtBuilder::build_default().as_externality();
        ext.execute_with(|| {
            let context = Context::default();
            context.setup();
            let proof = context.create_signed_transfer_fiat_nft_proof();
            let call = Box::new(MockCall::NftManager(
                super::Call::<TestRuntime>::signed_transfer_fiat_nft {
                    proof: proof.clone(),
                    nft_id: context.nft_id,
                    t2_transfer_to_public_key: context.t2_transfer_to_public_key,
                },
            ));

            let result = NftManager::get_proof(&call);

            assert!(result.is_ok());
            assert_eq!(result.unwrap(), proof);
        });
    }

    #[test]
    fn fails_for_invalid_calls() {
        let mut ext = ExtBuilder::build_default().as_externality();
        ext.execute_with(|| {
            let invalid_call = MockCall::System(frame_system::Call::remark { remark: vec![] });

            assert!(matches!(
                NftManager::get_proof(&invalid_call),
                Err(Error::<TestRuntime>::TransactionNotSupported)
            ));
        });
    }
}

fn create_proof_for_signed_transfer_fiat_nft(
    relayer: &AccountId,
    nft_owner_account: &AccountId,
    nft_owner_key_pair: &Pair,
    nft_id: &U256,
    t2_transfer_to_public_key: &H256,
    op_id: &u64,
) -> Proof<Signature, AccountId> {
    let context = SIGNED_TRANSFER_FIAT_NFT_CONTEXT;
    let data_to_sign = (context, relayer, nft_id, t2_transfer_to_public_key, op_id);
    let signature = sign(nft_owner_key_pair, &data_to_sign.encode());

    return build_proof(nft_owner_account, relayer, signature)
}
