//! # avn-proxy
// Copyright 2021 Aventus Network Systems (UK) Ltd.

//! avn-proxy pallet benchmarking.

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::{benchmarks, impl_benchmark_test_suite, whitelisted_caller};
use hex_literal::hex;
use sp_core::{sr25519, H256};

use crate::Pallet as AvnProxy;

fn get_proof<T: Config>(
    signer: T::AccountId,
    relayer: T::AccountId,
    signature: sr25519::Signature,
) -> Proof<T::Signature, T::AccountId> {
    return Proof { signer, relayer, signature: signature.into() }
}

fn get_payment_info<T: Config>(
    payer: T::AccountId,
    recipient: T::AccountId,
    amount: BalanceOf<T>,
    signature: T::Signature,
) -> PaymentInfo<T::AccountId, BalanceOf<T>, T::Signature> {
    return PaymentInfo { payer, recipient, amount, signature }
}

fn setup_balances<T: Config>(account: T::AccountId, amount: BalanceOf<T>) {
    // setup avt balance
    T::Currency::make_free_balance_be(&account, amount.into());
}

fn get_inner_call_proof<T: Config>(
    recipient: &T::AccountId,
    amount: BalanceOf<T>,
) -> (Proof<T::Signature, T::AccountId>, PaymentInfo<T::AccountId, BalanceOf<T>, T::Signature>) {
    let signer_account_raw: H256 =
        H256(hex!("482eae97356cdfd3b12774db1e5950471504d28b89aa169179d6c0527a04de23"));
    let signer =
        T::AccountId::decode(&mut signer_account_raw.as_bytes()).expect("valid account id");
    let inner_call_signature: sr25519::Signature = sr25519::Signature::from_slice(&hex!("a6350211fcdf1d7f0c79bf0a9c296de17449ca88a899f0cd19a70b07513fc107b7d34249dba71d4761ceeec2ed6bc1305defeb96418e6869e6b6199ed0de558e")).unwrap().into();
    let proof = get_proof::<T>(signer.clone(), recipient.clone(), inner_call_signature);

    #[cfg(test)]
    let signature: sr25519::Signature = sr25519::Signature::from_slice(&hex!("98dca66ceef8a68d6df1a3b989ea6f80337447753091844f5be8f5dcdf694338b94a5335f0297b005252d712d89ced7450755823b9dde5b1ffd57708a2c1ad81")).unwrap().into();
    #[cfg(not(test))]
    let signature: sr25519::Signature = sr25519::Signature::from_slice(&hex!("4cf3364106905fa0caba16d93f1ca4b5afa64d37ef70e2b1dc0b95972183af025f977aa29012d4a19dce4869ded87ab4659f1f3ee05d79b6fb9723dac262418b")).unwrap().into();

    let payment_authorisation =
        get_payment_info::<T>(signer.clone(), recipient.clone(), amount, signature.into());

    setup_balances::<T>(signer, amount);

    return (proof, payment_authorisation)
}

benchmarks! {
    charge_fee {
        let recipient: T::AccountId = whitelisted_caller();
        let amount: BalanceOf<T> = 10u32.into();

        let (proof, payment_authorisation) = get_inner_call_proof::<T>(&recipient, amount);
    }: {
        AvnProxy::<T>::charge_fee(&proof, payment_authorisation)?;
    }
    verify {
        assert_eq!(T::Currency::free_balance(&recipient), amount.into());
    }
}

impl_benchmark_test_suite!(
    Pallet,
    crate::mock::ExtBuilder::build_default().with_balances().as_externality(),
    crate::mock::TestRuntime,
);
