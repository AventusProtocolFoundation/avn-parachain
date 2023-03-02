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
use crate::{
    self as token_manager,
    mock::{Balances, Event, *},
    *,
};
use codec::Encode;
use frame_support::{assert_err, assert_noop, assert_ok};
use pallet_parachain_staking::Weight;
use pallet_transaction_payment::ChargeTransactionPayment;
use sp_core::{sr25519, Pair};
use sp_runtime::{
    traits::{Hash, SignedExtension},
    transaction_validity::InvalidTransaction,
};

type AccountId = <TestRuntime as frame_system::Config>::AccountId;
type Hashing = <TestRuntime as frame_system::Config>::Hashing;

const DEFAULT_AMOUNT: u128 = 1_000_000;
const DEFAULT_NONCE: u64 = 0;
const NON_ZERO_NONCE: u64 = 100;

pub static TX_LEN: usize = 1;

pub fn default_key_pair() -> sr25519::Pair {
    return sr25519::Pair::from_seed(&[70u8; 32])
}

fn default_sender() -> AccountId {
    return AccountId::decode(&mut default_key_pair().public().to_vec().as_slice()).unwrap()
}

fn default_receiver() -> AccountId {
    return AccountId::from_raw([1; 32])
}

fn default_relayer() -> AccountId {
    return AccountId::from_raw([10; 32])
}

fn pay_gas_and_proxy_call(
    relayer: &AccountId,
    outer_call: &<TestRuntime as frame_system::Config>::Call,
    inner_call: Box<<TestRuntime as Config>::Call>,
) -> DispatchResult {
    // See: /primitives/runtime/src/traits.rs for more details
    <ChargeTransactionPayment<TestRuntime> as SignedExtension>::pre_dispatch(
        ChargeTransactionPayment::from(0), // we do not pay any tip
        relayer,
        outer_call,
        &info_from_weight(Weight::from_ref_time(1)),
        TX_LEN,
    )
    .map_err(|e| <&'static str>::from(e))?;

    return TokenManager::proxy(Origin::signed(*relayer), inner_call)
}

fn pay_gas_and_call_transfer_directly(
    sender: &AccountId,
    receiver: &AccountId,
    token_id: <TestRuntime as Config>::TokenId,
    amount: <TestRuntime as Config>::TokenBalance,
    proof: Proof<Signature, AccountId>,
    call: &<TestRuntime as frame_system::Config>::Call,
) -> DispatchResult {
    <ChargeTransactionPayment<TestRuntime> as SignedExtension>::pre_dispatch(
        ChargeTransactionPayment::from(0),
        sender,
        call,
        &info_from_weight(Weight::from_ref_time(1)),
        TX_LEN,
    )
    .map_err(|e| <&'static str>::from(e))?;

    return TokenManager::signed_transfer(
        Origin::signed(*sender),
        proof,
        *sender,
        *receiver,
        token_id,
        amount,
    )
}

fn build_proof(
    signer: &AccountId,
    relayer: &AccountId,
    signature: Signature,
) -> Proof<Signature, AccountId> {
    return Proof { signer: *signer, relayer: *relayer, signature }
}

fn setup(sender: &AccountId, nonce: u64) {
    <TokenManager as Store>::Balances::insert((NON_AVT_TOKEN_ID, sender), 2 * DEFAULT_AMOUNT);
    <TokenManager as Store>::Nonces::insert(sender, nonce);
}

fn default_setup() {
    setup(&default_sender(), DEFAULT_NONCE);
}

fn create_proof_for_signed_transfer_with_relayer(
    relayer: &AccountId,
) -> Proof<Signature, AccountId> {
    return create_proof_for_signed_transfer(
        relayer,
        &default_sender(),
        &default_receiver(),
        NON_AVT_TOKEN_ID,
        DEFAULT_AMOUNT,
        DEFAULT_NONCE,
        &default_key_pair(),
    )
}

fn create_proof_for_signed_transfer_with_nonce(nonce: u64) -> Proof<Signature, AccountId> {
    return create_proof_for_signed_transfer(
        &default_relayer(),
        &default_sender(),
        &default_receiver(),
        NON_AVT_TOKEN_ID,
        DEFAULT_AMOUNT,
        nonce,
        &default_key_pair(),
    )
}

fn create_default_proof_for_signed_transfer() -> Proof<Signature, AccountId> {
    return create_proof_for_signed_transfer(
        &default_relayer(),
        &default_sender(),
        &default_receiver(),
        NON_AVT_TOKEN_ID,
        DEFAULT_AMOUNT,
        DEFAULT_NONCE,
        &default_key_pair(),
    )
}

fn create_proof_for_signed_transfer(
    relayer: &AccountId,
    from: &AccountId,
    to: &AccountId,
    token_id: H160,
    amount: u128,
    nonce: u64,
    keys: &sr25519::Pair,
) -> Proof<Signature, AccountId> {
    let context = SIGNED_TRANSFER_CONTEXT;
    let data_to_sign = (context, relayer, from, to, token_id, amount, nonce);
    let signature = sign(&keys, &data_to_sign.encode());

    return build_proof(from, relayer, signature)
}

fn check_proxy_transfer_default_call_succeed(call: Box<<TestRuntime as Config>::Call>) {
    let call_hash = Hashing::hash_of(&call);

    assert_ok!(TokenManager::proxy(Origin::signed(default_relayer()), call));
    assert_eq!(System::events().len(), 2);
    assert!(System::events().iter().any(|a| a.event ==
        Event::TokenManager(crate::Event::<TestRuntime>::CallDispatched {
            relayer: default_relayer(),
            call_hash
        })));
    assert!(System::events().iter().any(|a| a.event ==
        Event::TokenManager(crate::Event::<TestRuntime>::TokenTransferred {
            token_id: NON_AVT_TOKEN_ID,
            sender: default_sender(),
            recipient: default_receiver(),
            token_balance: DEFAULT_AMOUNT
        })));
}

#[test]
fn avn_test_proxy_signed_transfer_succeeds() {
    let mut ext = ExtBuilder::build_default().as_externality();
    ext.execute_with(|| {
        let sender = default_sender();
        let relayer = default_relayer();
        let recipient = default_receiver();

        setup(&sender, NON_ZERO_NONCE);
        let proof = create_proof_for_signed_transfer_with_nonce(NON_ZERO_NONCE);

        let call =
            Box::new(mock::Call::TokenManager(super::Call::<TestRuntime>::signed_transfer {
                proof,
                from: sender,
                to: recipient,
                token_id: NON_AVT_TOKEN_ID,
                amount: DEFAULT_AMOUNT,
            }));
        let call_hash = Hashing::hash_of(&call);

        assert_eq!(System::events().len(), 0);
        assert_ok!(TokenManager::proxy(Origin::signed(relayer), call));

        assert_eq!(
            <TokenManager as Store>::Balances::get((NON_AVT_TOKEN_ID, sender)),
            DEFAULT_AMOUNT
        );
        assert_eq!(
            <TokenManager as Store>::Balances::get((NON_AVT_TOKEN_ID, recipient)),
            DEFAULT_AMOUNT
        );

        assert!(System::events().iter().any(|a| a.event ==
            Event::TokenManager(crate::Event::<TestRuntime>::CallDispatched {
                relayer,
                call_hash
            })));

        // In order to validate that the proxied call has been dispatched we need any proof that
        // transfer was called. In this case we will check that the Transferred signal was
        // emitted.
        assert!(System::events().iter().any(|a| a.event ==
            Event::TokenManager(crate::Event::<TestRuntime>::TokenTransferred {
                token_id: NON_AVT_TOKEN_ID,
                sender,
                recipient,
                token_balance: DEFAULT_AMOUNT
            })));
    });
}

#[test]
fn avt_proxy_signed_transfer_succeeds() {
    let mut ext = ExtBuilder::build_default()
        .with_genesis_config()
        .with_balances()
        .as_externality();

    ext.execute_with(|| {
        let sender = account_id_with_100_avt();
        let relayer = account_id2_with_100_avt();
        let recipient = default_receiver();

        let sender_init_avt_balance = Balances::free_balance(sender);
        let recipient_init_avt_balance = Balances::free_balance(recipient);
        let init_total_avt_issuance = Balances::total_issuance();

        setup(&sender, NON_ZERO_NONCE);
        let proof = create_proof_for_signed_transfer(
            &relayer,
            &sender,
            &recipient,
            AVT_TOKEN_CONTRACT,
            DEFAULT_AMOUNT,
            NON_ZERO_NONCE,
            &key_pair_for_account_with_100_avt(),
        );

        let call =
            Box::new(mock::Call::TokenManager(super::Call::<TestRuntime>::signed_transfer {
                proof,
                from: sender,
                to: recipient,
                token_id: AVT_TOKEN_CONTRACT,
                amount: DEFAULT_AMOUNT,
            }));
        let call_hash = Hashing::hash_of(&call);

        assert_eq!(System::events().len(), 0);
        assert_ok!(TokenManager::proxy(Origin::signed(relayer), call));

        // Show that we have transferred not created or removed avt from the system
        assert_eq!(init_total_avt_issuance, Balances::total_issuance());

        // Check sender and recipient balances after the transfer amount
        assert_eq!(sender_init_avt_balance - DEFAULT_AMOUNT, Balances::free_balance(sender));
        assert_eq!(recipient_init_avt_balance + DEFAULT_AMOUNT, Balances::free_balance(recipient));

        // Check that the token manager nonce increases
        assert_eq!(NON_ZERO_NONCE + 1, <TokenManager as Store>::Nonces::get(sender));

        // Check for events
        assert!(System::events().iter().any(|a| a.event ==
            Event::TokenManager(crate::Event::<TestRuntime>::CallDispatched {
                relayer,
                call_hash
            })));

        // In order to validate that the proxied call has been dispatched we need any proof that
        // transfer was called. In this case we will check that the Transferred signal was
        // emitted.
        assert!(System::events().iter().any(|a| a.event ==
            Event::TokenManager(crate::Event::<TestRuntime>::TokenTransferred {
                token_id: AVT_TOKEN_CONTRACT,
                sender,
                recipient,
                token_balance: DEFAULT_AMOUNT
            })));
    });
}

#[test]
fn avn_test_direct_signed_transfer_succeeds() {
    let mut ext = ExtBuilder::build_default().as_externality();
    ext.execute_with(|| {
        let sender = default_sender();
        let recipient = default_receiver(); // just some arbitrary account id

        setup(&sender, NON_ZERO_NONCE);
        let proof = create_proof_for_signed_transfer_with_nonce(NON_ZERO_NONCE);

        assert_eq!(System::events().len(), 0);
        assert_ok!(TokenManager::signed_transfer(
            Origin::signed(sender),
            proof,
            sender,
            recipient,
            NON_AVT_TOKEN_ID,
            DEFAULT_AMOUNT
        ));

        assert_eq!(
            <TokenManager as Store>::Balances::get((NON_AVT_TOKEN_ID, sender)),
            DEFAULT_AMOUNT
        );
        assert_eq!(
            <TokenManager as Store>::Balances::get((NON_AVT_TOKEN_ID, recipient)),
            DEFAULT_AMOUNT
        );
        assert!(System::events().iter().any(|a| a.event ==
            Event::TokenManager(crate::Event::<TestRuntime>::TokenTransferred {
                token_id: NON_AVT_TOKEN_ID,
                sender,
                recipient,
                token_balance: DEFAULT_AMOUNT
            })));
    });
}

#[test]
fn avt_direct_signed_transfer_succeeds() {
    let mut ext = ExtBuilder::build_default()
        .with_genesis_config()
        .with_balances()
        .as_externality();

    ext.execute_with(|| {
        let sender = account_id_with_100_avt();
        let relayer = account_id2_with_100_avt();
        let recipient = default_receiver(); // just some arbitrary account id

        let sender_init_avt_balance = Balances::free_balance(sender);
        let recipient_init_avt_balance = Balances::free_balance(recipient);
        let init_total_avt_issuance = Balances::total_issuance();

        setup(&sender, NON_ZERO_NONCE);
        let proof = create_proof_for_signed_transfer(
            &relayer,
            &sender,
            &recipient,
            AVT_TOKEN_CONTRACT,
            DEFAULT_AMOUNT,
            NON_ZERO_NONCE,
            &key_pair_for_account_with_100_avt(),
        );

        assert_eq!(System::events().len(), 0);
        assert_ok!(TokenManager::signed_transfer(
            Origin::signed(sender),
            proof,
            sender,
            recipient,
            AVT_TOKEN_CONTRACT,
            DEFAULT_AMOUNT
        ));

        assert_eq!(init_total_avt_issuance, Balances::total_issuance());
        assert_eq!(sender_init_avt_balance - DEFAULT_AMOUNT, Balances::free_balance(sender));
        assert_eq!(recipient_init_avt_balance + DEFAULT_AMOUNT, Balances::free_balance(recipient));

        // Check that the token manager nonce increases
        assert_eq!(NON_ZERO_NONCE + 1, <TokenManager as Store>::Nonces::get(sender));

        assert!(System::events().iter().any(|a| a.event ==
            Event::TokenManager(crate::Event::<TestRuntime>::TokenTransferred {
                token_id: AVT_TOKEN_CONTRACT,
                sender,
                recipient,
                token_balance: DEFAULT_AMOUNT
            })));
    });
}

#[test]
fn avn_test_proxy_signed_transfer_succeeds_with_nonce_zero() {
    let mut ext = ExtBuilder::build_default().as_externality();
    ext.execute_with(|| {
        let sender = default_sender();
        let relayer = default_relayer();
        let recipient = default_receiver();

        default_setup();
        let proof = create_default_proof_for_signed_transfer();

        let call =
            Box::new(mock::Call::TokenManager(super::Call::<TestRuntime>::signed_transfer {
                proof,
                from: sender,
                to: recipient,
                token_id: NON_AVT_TOKEN_ID,
                amount: DEFAULT_AMOUNT,
            }));
        let call_hash = Hashing::hash_of(&call);

        assert_eq!(System::events().len(), 0);
        assert_ok!(TokenManager::proxy(Origin::signed(relayer), call));

        assert_eq!(
            <TokenManager as Store>::Balances::get((NON_AVT_TOKEN_ID, sender)),
            DEFAULT_AMOUNT
        );
        assert_eq!(
            <TokenManager as Store>::Balances::get((NON_AVT_TOKEN_ID, recipient)),
            DEFAULT_AMOUNT
        );

        assert!(System::events().iter().any(|a| a.event ==
            Event::TokenManager(crate::Event::<TestRuntime>::CallDispatched {
                relayer,
                call_hash
            })));
        assert!(System::events().iter().any(|a| a.event ==
            Event::TokenManager(crate::Event::<TestRuntime>::TokenTransferred {
                token_id: NON_AVT_TOKEN_ID,
                sender,
                recipient,
                token_balance: DEFAULT_AMOUNT
            })));
    });
}

#[test]
fn avn_test_proxy_signed_transfer_fails_for_mismatching_proof_nonce() {
    let mut ext = ExtBuilder::build_default().as_externality();
    ext.execute_with(|| {
        let sender = default_sender();
        let relayer = default_relayer();
        let recipient = default_receiver();

        let bad_nonces = [0, 99, 101];

        setup(&sender, NON_ZERO_NONCE);

        for bad_nonce in bad_nonces.iter() {
            let proof = create_proof_for_signed_transfer_with_nonce(*bad_nonce);
            let call =
                Box::new(mock::Call::TokenManager(super::Call::<TestRuntime>::signed_transfer {
                    proof,
                    from: sender,
                    to: recipient,
                    token_id: NON_AVT_TOKEN_ID,
                    amount: DEFAULT_AMOUNT,
                }));

            assert_err!(
                TokenManager::proxy(Origin::signed(relayer), call),
                Error::<TestRuntime>::UnauthorizedSignedTransferTransaction
            );

            assert_eq!(System::events().len(), 0);
        }
    });
}

#[test]
fn avn_test_proxy_signed_transfer_fails_with_mismatched_proof_other_amount() {
    let mut ext = ExtBuilder::build_default().as_externality();
    ext.execute_with(|| {
        let sender = default_sender();
        let relayer = default_relayer();
        let recipient = default_receiver();

        let mismatching_amount: u128 = 100;

        default_setup();
        let proof = create_default_proof_for_signed_transfer();

        let call =
            Box::new(mock::Call::TokenManager(super::Call::<TestRuntime>::signed_transfer {
                proof: proof.clone(),
                from: sender,
                to: recipient,
                token_id: NON_AVT_TOKEN_ID,
                amount: mismatching_amount,
            }));

        assert_eq!(System::events().len(), 0);
        assert_err!(
            TokenManager::proxy(Origin::signed(relayer), call),
            Error::<TestRuntime>::UnauthorizedSignedTransferTransaction
        );

        // Show that it works with the correct input
        let proof = create_default_proof_for_signed_transfer();
        check_proxy_transfer_default_call_succeed(Box::new(mock::Call::TokenManager(
            super::Call::<TestRuntime>::signed_transfer {
                proof,
                from: sender,
                to: recipient,
                token_id: NON_AVT_TOKEN_ID,
                amount: DEFAULT_AMOUNT,
            },
        )));
    });
}

#[test]
fn avn_test_proxy_signed_transfer_fails_with_mismatched_proof_other_keys() {
    let mut ext = ExtBuilder::build_default().as_externality();
    ext.execute_with(|| {
        let sender = default_sender();
        let relayer = default_relayer();
        let recipient = default_receiver();

        let other_sender_keys = sr25519::Pair::from_entropy(&[2u8; 32], None).0;

        default_setup();
        let mismatching_proof = create_proof_for_signed_transfer(
            &relayer,
            &sender,
            &recipient,
            NON_AVT_TOKEN_ID,
            DEFAULT_AMOUNT,
            DEFAULT_NONCE,
            &other_sender_keys,
        );

        let call =
            Box::new(mock::Call::TokenManager(super::Call::<TestRuntime>::signed_transfer {
                proof: mismatching_proof,
                from: sender,
                to: recipient,
                token_id: NON_AVT_TOKEN_ID,
                amount: DEFAULT_AMOUNT,
            }));

        assert_err!(
            TokenManager::proxy(Origin::signed(relayer), call),
            Error::<TestRuntime>::UnauthorizedSignedTransferTransaction
        );
        assert_eq!(System::events().len(), 0);

        // Show that it works with the correct input
        let proof = create_default_proof_for_signed_transfer();
        check_proxy_transfer_default_call_succeed(Box::new(mock::Call::TokenManager(
            super::Call::<TestRuntime>::signed_transfer {
                proof,
                from: sender,
                to: recipient,
                token_id: NON_AVT_TOKEN_ID,
                amount: DEFAULT_AMOUNT,
            },
        )));
    });
}

#[test]
fn avn_test_proxy_signed_transfer_fails_with_mismatched_proof_other_sender() {
    let mut ext = ExtBuilder::build_default().as_externality();
    ext.execute_with(|| {
        let sender_keys = default_key_pair();
        let sender = default_sender();
        let relayer = default_relayer();
        let recipient = default_receiver();

        let other_sender_account_id = AccountId::from_raw([55; 32]);

        default_setup();
        let mismatching_proof = create_proof_for_signed_transfer(
            &relayer,
            &other_sender_account_id,
            &recipient,
            NON_AVT_TOKEN_ID,
            DEFAULT_AMOUNT,
            DEFAULT_NONCE,
            &sender_keys,
        );

        let call =
            Box::new(mock::Call::TokenManager(super::Call::<TestRuntime>::signed_transfer {
                proof: mismatching_proof,
                from: sender,
                to: recipient,
                token_id: NON_AVT_TOKEN_ID,
                amount: DEFAULT_AMOUNT,
            }));

        assert_err!(
            TokenManager::proxy(Origin::signed(relayer), call),
            Error::<TestRuntime>::SenderNotValid
        );
        assert_eq!(System::events().len(), 0);

        // Show that it works with the correct input
        let proof = create_default_proof_for_signed_transfer();
        check_proxy_transfer_default_call_succeed(Box::new(mock::Call::TokenManager(
            super::Call::<TestRuntime>::signed_transfer {
                proof,
                from: sender,
                to: recipient,
                token_id: NON_AVT_TOKEN_ID,
                amount: DEFAULT_AMOUNT,
            },
        )));
    });
}

#[test]
fn avn_test_proxy_signed_transfer_fails_with_mismatched_proof_other_relayer() {
    let mut ext = ExtBuilder::build_default().as_externality();
    ext.execute_with(|| {
        let sender = default_sender();
        let relayer = default_relayer();
        let recipient = default_receiver();

        let other_relayer = recipient.clone();

        default_setup();
        let mismatching_proof = create_proof_for_signed_transfer_with_relayer(&other_relayer);
        let call =
            Box::new(mock::Call::TokenManager(super::Call::<TestRuntime>::signed_transfer {
                proof: mismatching_proof,
                from: sender,
                to: recipient,
                token_id: NON_AVT_TOKEN_ID,
                amount: DEFAULT_AMOUNT,
            }));

        assert_err!(
            TokenManager::proxy(Origin::signed(relayer), call.clone()),
            Error::<TestRuntime>::UnauthorizedProxyTransaction
        );
        assert_eq!(System::events().len(), 0);

        // Show that it works with the correct input
        let proof = create_default_proof_for_signed_transfer();
        check_proxy_transfer_default_call_succeed(Box::new(mock::Call::TokenManager(
            super::Call::<TestRuntime>::signed_transfer {
                proof,
                from: sender,
                to: recipient,
                token_id: NON_AVT_TOKEN_ID,
                amount: DEFAULT_AMOUNT,
            },
        )));
    });
}

#[test]
fn avn_test_proxy_signed_transfer_fails_with_mismatched_proof_other_recipient() {
    let mut ext = ExtBuilder::build_default().as_externality();
    ext.execute_with(|| {
        let sender_keys = default_key_pair();
        let sender = default_sender();
        let relayer = default_relayer();
        let recipient = default_receiver();

        let other_recipient_account_id = AccountId::from_raw([4; 32]);

        default_setup();
        let mismatching_proof = create_proof_for_signed_transfer(
            &relayer,
            &sender,
            &other_recipient_account_id,
            NON_AVT_TOKEN_ID,
            DEFAULT_AMOUNT,
            DEFAULT_NONCE,
            &sender_keys,
        );

        let call =
            Box::new(mock::Call::TokenManager(super::Call::<TestRuntime>::signed_transfer {
                proof: mismatching_proof,
                from: sender,
                to: recipient,
                token_id: NON_AVT_TOKEN_ID,
                amount: DEFAULT_AMOUNT,
            }));

        assert_err!(
            TokenManager::proxy(Origin::signed(relayer), call),
            Error::<TestRuntime>::UnauthorizedSignedTransferTransaction
        );
        assert_eq!(System::events().len(), 0);

        // Show that it works with the correct input
        let proof = create_default_proof_for_signed_transfer();
        check_proxy_transfer_default_call_succeed(Box::new(mock::Call::TokenManager(
            super::Call::<TestRuntime>::signed_transfer {
                proof,
                from: sender,
                to: recipient,
                token_id: NON_AVT_TOKEN_ID,
                amount: DEFAULT_AMOUNT,
            },
        )));
    });
}

#[test]
fn avn_test_proxy_signed_transfer_fails_with_mismatched_proof_other_token_id() {
    let mut ext = ExtBuilder::build_default().as_externality();
    ext.execute_with(|| {
        let sender_keys = default_key_pair();
        let sender = default_sender();
        let relayer = default_relayer();
        let recipient = default_receiver();

        let other_token_id = NON_AVT_TOKEN_ID_2;

        default_setup();
        let mismatching_proof = create_proof_for_signed_transfer(
            &relayer,
            &sender,
            &recipient,
            other_token_id,
            DEFAULT_AMOUNT,
            DEFAULT_NONCE,
            &sender_keys,
        );

        let call =
            Box::new(mock::Call::TokenManager(super::Call::<TestRuntime>::signed_transfer {
                proof: mismatching_proof,
                from: sender,
                to: recipient,
                token_id: NON_AVT_TOKEN_ID,
                amount: DEFAULT_AMOUNT,
            }));

        assert_err!(
            TokenManager::proxy(Origin::signed(relayer), call),
            Error::<TestRuntime>::UnauthorizedSignedTransferTransaction
        );
        assert_eq!(System::events().len(), 0);

        // Show that it works with the correct input
        let proof = create_default_proof_for_signed_transfer();
        check_proxy_transfer_default_call_succeed(Box::new(mock::Call::TokenManager(
            super::Call::<TestRuntime>::signed_transfer {
                proof,
                from: sender,
                to: recipient,
                token_id: NON_AVT_TOKEN_ID,
                amount: DEFAULT_AMOUNT,
            },
        )));
    });
}

#[test]
fn avn_test_proxy_signed_transfer_fails_with_mismatched_proof_other_context() {
    let mut ext = ExtBuilder::build_default().as_externality();
    ext.execute_with(|| {
        let sender_keys = default_key_pair();
        let sender = default_sender();
        let relayer = default_relayer();
        let recipient = default_receiver();

        let other_context: &'static [u8] = b"authorizati0n for tr4nsfer op3ration";

        default_setup();
        let data_to_sign = (
            other_context,
            relayer,
            sender,
            recipient,
            NON_AVT_TOKEN_ID,
            DEFAULT_AMOUNT,
            DEFAULT_NONCE,
        );
        let alternative_signature = sign(&sender_keys, &data_to_sign.encode());

        let mismatching_proof = Proof { signer: sender, relayer, signature: alternative_signature };

        let call =
            Box::new(mock::Call::TokenManager(super::Call::<TestRuntime>::signed_transfer {
                proof: mismatching_proof,
                from: sender,
                to: recipient,
                token_id: NON_AVT_TOKEN_ID,
                amount: DEFAULT_AMOUNT,
            }));
        assert_err!(
            TokenManager::proxy(Origin::signed(relayer), call),
            Error::<TestRuntime>::UnauthorizedSignedTransferTransaction
        );

        assert_eq!(System::events().len(), 0);

        // Show that it works with the correct input
        let proof = create_default_proof_for_signed_transfer();
        check_proxy_transfer_default_call_succeed(Box::new(mock::Call::TokenManager(
            super::Call::<TestRuntime>::signed_transfer {
                proof,
                from: sender,
                to: recipient,
                token_id: NON_AVT_TOKEN_ID,
                amount: DEFAULT_AMOUNT,
            },
        )));
    });
}

#[test]
fn avn_test_unsupported_proxy_call() {
    let mut ext = ExtBuilder::build_default().as_externality();
    ext.execute_with(|| {
        let call = Box::new(mock::Call::System(frame_system::Call::remark { remark: vec![] }));
        assert_noop!(
            TokenManager::proxy(Origin::signed(default_sender()), call),
            Error::<TestRuntime>::TransactionNotSupported
        );
    });
}

// ----------------------------- Get Proof tests------------------------------------------

#[test]
fn avn_test_get_proof_succeeds_for_valid_cases() {
    let mut ext = ExtBuilder::build_default().as_externality();
    ext.execute_with(|| {
        let sender = default_sender();
        let recipient = default_receiver();

        let proof = create_default_proof_for_signed_transfer();
        let call =
            Box::new(mock::Call::TokenManager(super::Call::<TestRuntime>::signed_transfer {
                proof: proof.clone(),
                from: sender,
                to: recipient,
                token_id: NON_AVT_TOKEN_ID,
                amount: DEFAULT_AMOUNT,
            }));

        let result = TokenManager::get_proof(&call);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), proof);
    });
}

#[test]
fn avn_test_get_proof_fails_for_invalid_calls() {
    let mut ext = ExtBuilder::build_default().as_externality();
    ext.execute_with(|| {
        let invalid_call = mock::Call::System(frame_system::Call::remark { remark: vec![] });

        assert!(matches!(
            TokenManager::get_proof(&invalid_call),
            Err(Error::<TestRuntime>::TransactionNotSupported)
        ));
    });
}

// ----------------------------- funds related tests -----------------------------------
#[test]
// Ensure that the AVT gas fees are payed by the relayer
fn avn_test_proxy_gas_costs_paid_correctly() {
    let mut ext = ExtBuilder::build_default().with_balances().as_externality();

    ext.execute_with(|| {
        let relayer = account_id_with_100_avt();
        let sender = default_sender();
        let recipient = default_receiver();

        default_setup();
        let proof = create_proof_for_signed_transfer_with_relayer(&relayer);

        assert_eq!(Balances::free_balance(relayer), AMOUNT_100_TOKEN);
        assert_eq!(Balances::free_balance(sender), 0);
        assert_eq!(Balances::free_balance(recipient), 0);
        assert_eq!(
            <TokenManager as Store>::Balances::get((NON_AVT_TOKEN_ID, sender)),
            2 * DEFAULT_AMOUNT
        );
        assert_eq!(<TokenManager as Store>::Balances::get((NON_AVT_TOKEN_ID, recipient)), 0);

        // Prepare the calls
        let inner_call =
            Box::new(mock::Call::TokenManager(super::Call::<TestRuntime>::signed_transfer {
                proof: proof.clone(),
                from: sender,
                to: recipient,
                token_id: NON_AVT_TOKEN_ID,
                amount: DEFAULT_AMOUNT,
            }));
        let outer_call =
            &mock::Call::TokenManager(token_manager::Call::proxy { call: inner_call.clone() });

        // Pay fees and submit the transaction
        assert_ok!(pay_gas_and_proxy_call(&relayer, outer_call, inner_call.clone()));

        // Check the effects of the transaction
        let call_hash = Hashing::hash_of(&inner_call);
        assert!(System::events().iter().any(|a| a.event ==
            Event::TokenManager(crate::Event::<TestRuntime>::CallDispatched {
                relayer,
                call_hash
            })));

        assert!(System::events().iter().any(|a| a.event ==
            Event::TokenManager(crate::Event::<TestRuntime>::TokenTransferred {
                token_id: NON_AVT_TOKEN_ID,
                sender,
                recipient,
                token_balance: DEFAULT_AMOUNT
            })));

        let fee: u128 = (BASE_FEE + TX_LEN as u64) as u128;
        assert_eq!(Balances::free_balance(relayer), AMOUNT_100_TOKEN - fee);
        assert_eq!(Balances::free_balance(sender), 0);
        assert_eq!(Balances::free_balance(recipient), 0);
        assert_eq!(
            <TokenManager as Store>::Balances::get((NON_AVT_TOKEN_ID, sender)),
            DEFAULT_AMOUNT
        );
        assert_eq!(
            <TokenManager as Store>::Balances::get((NON_AVT_TOKEN_ID, recipient)),
            DEFAULT_AMOUNT
        );
    });
}

#[test]
// Ensure that regular call's gas fees are payed by the sender
fn avn_test_regular_call_gas_costs_paid_correctly() {
    let mut ext = ExtBuilder::build_default().with_balances().as_externality();

    ext.execute_with(|| {
        let sender_keys = key_pair_for_account_with_100_avt();
        let sender = get_account_id(&sender_keys);
        let recipient = default_receiver();

        setup(&sender, DEFAULT_NONCE);
        let proof = create_proof_for_signed_transfer(
            &sender,
            &sender,
            &recipient,
            NON_AVT_TOKEN_ID,
            DEFAULT_AMOUNT,
            DEFAULT_NONCE,
            &sender_keys,
        );

        assert_eq!(Balances::free_balance(sender), AMOUNT_100_TOKEN);
        assert_eq!(
            <TokenManager as Store>::Balances::get((NON_AVT_TOKEN_ID, sender)),
            2 * DEFAULT_AMOUNT
        );
        assert_eq!(<TokenManager as Store>::Balances::get((NON_AVT_TOKEN_ID, recipient)), 0);
        assert_eq!(System::events().len(), 0);

        let call = &mock::Call::TokenManager(token_manager::Call::signed_transfer {
            proof: proof.clone(),
            from: sender,
            to: recipient,
            token_id: NON_AVT_TOKEN_ID,
            amount: DEFAULT_AMOUNT,
        });
        assert_ok!(pay_gas_and_call_transfer_directly(
            &sender,
            &recipient,
            NON_AVT_TOKEN_ID,
            DEFAULT_AMOUNT,
            proof,
            call
        ));

        let fee: u128 = (BASE_FEE + TX_LEN as u64) as u128;
        assert_eq!(System::events().len(), 2);
        assert_eq!(Balances::free_balance(sender), AMOUNT_100_TOKEN - fee);
        assert_eq!(
            <TokenManager as Store>::Balances::get((NON_AVT_TOKEN_ID, sender)),
            DEFAULT_AMOUNT
        );
        assert_eq!(
            <TokenManager as Store>::Balances::get((NON_AVT_TOKEN_ID, recipient)),
            DEFAULT_AMOUNT
        );
        assert!(System::events().iter().any(|a| a.event ==
            Event::TokenManager(crate::Event::<TestRuntime>::TokenTransferred {
                token_id: NON_AVT_TOKEN_ID,
                sender,
                recipient,
                token_balance: DEFAULT_AMOUNT
            })));
    });
}

#[test]
// Relayer has insufficient funds to send transaction
fn avn_test_proxy_insufficient_avt() {
    let mut ext = ExtBuilder::build_default().with_balances().as_externality();

    ext.execute_with(|| {
        let relayer = default_relayer();
        let sender = default_sender();
        let recipient = default_receiver();

        default_setup();
        let proof = create_default_proof_for_signed_transfer();

        assert_eq!(Balances::free_balance(relayer), 0);
        assert_eq!(
            <TokenManager as Store>::Balances::get((NON_AVT_TOKEN_ID, sender)),
            2 * DEFAULT_AMOUNT
        );

        // Prepare the calls
        let inner_call =
            Box::new(mock::Call::TokenManager(super::Call::<TestRuntime>::signed_transfer {
                proof: proof.clone(),
                from: sender,
                to: recipient,
                token_id: NON_AVT_TOKEN_ID,
                amount: DEFAULT_AMOUNT,
            }));
        let outer_call =
            &mock::Call::TokenManager(token_manager::Call::proxy { call: inner_call.clone() });

        // Pay fees and submit the transaction.
        // Gas fee for this tx is (BASE_FEE + TX_LEN): 12 + 1 = 13 AVT
        assert_noop!(
            pay_gas_and_proxy_call(&relayer, outer_call, inner_call),
            <&str>::from(InvalidTransaction::Payment)
        );
        assert_eq!(System::events().len(), 0);
    });
}

#[test]
// Ensure that regular call's gas fees are payed by the sender
fn avn_test_regular_insufficient_avt() {
    let mut ext = ExtBuilder::build_default().with_balances().as_externality();

    ext.execute_with(|| {
        let sender = default_sender();
        let recipient = default_receiver();

        default_setup();
        let proof = create_default_proof_for_signed_transfer();

        assert_eq!(Balances::free_balance(sender), 0);
        assert_eq!(
            <TokenManager as Store>::Balances::get((NON_AVT_TOKEN_ID, sender)),
            2 * DEFAULT_AMOUNT
        );
        assert_eq!(<TokenManager as Store>::Balances::get((NON_AVT_TOKEN_ID, recipient)), 0);

        let call = &mock::Call::TokenManager(token_manager::Call::signed_transfer {
            proof: proof.clone(),
            from: sender,
            to: recipient,
            token_id: NON_AVT_TOKEN_ID,
            amount: DEFAULT_AMOUNT,
        });
        assert_noop!(
            pay_gas_and_call_transfer_directly(
                &sender,
                &recipient,
                NON_AVT_TOKEN_ID,
                DEFAULT_AMOUNT,
                proof,
                call
            ),
            <&str>::from(InvalidTransaction::Payment)
        );
        assert_eq!(System::events().len(), 0);
    });
}
