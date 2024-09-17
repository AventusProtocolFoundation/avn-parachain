//! # avn-proxy
// Copyright 2021 Aventus Network Systems (UK) Ltd.

//! avn-proxy pallet benchmarking.

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::Pallet as AvnProxy;
use frame_benchmarking::{benchmarks, impl_benchmark_test_suite, whitelisted_caller};
use hex_literal::hex;
use sp_core::{sr25519, H256};

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
    token: T::Token,
) -> PaymentInfo<T::AccountId, BalanceOf<T>, T::Signature, T::Token> {
    return PaymentInfo { payer, recipient, amount, signature, token }
}

fn setup_balances<T: Config>(account: T::AccountId, amount: BalanceOf<T>) {
    // setup avt balance
    T::Currency::make_free_balance_be(&account, amount.into());
}

fn get_inner_call_proof<T: Config>(
    recipient: &T::AccountId,
    amount: BalanceOf<T>,
    token: H160,
    signature: sr25519::Signature,
) -> (
    Proof<T::Signature, T::AccountId>,
    PaymentInfo<T::AccountId, BalanceOf<T>, T::Signature, T::Token>,
    T::AccountId,
) {
    #[cfg(test)]
    let signer = T::AccountId::decode(
        &mut (H256::from_slice(&crate::mock::get_default_signer().public_key()).as_bytes()),
    )
    .expect("valid account id");
    #[cfg(not(test))]
    let signer = T::AccountId::decode(
        &mut H256(hex!("482eae97356cdfd3b12774db1e5950471504d28b89aa169179d6c0527a04de23"))
            .as_bytes(),
    )
    .expect("valid account id");

    let inner_call_signature: sr25519::Signature = sr25519::Signature::from_slice(&hex!("a6350211fcdf1d7f0c79bf0a9c296de17449ca88a899f0cd19a70b07513fc107b7d34249dba71d4761ceeec2ed6bc1305defeb96418e6869e6b6199ed0de558e")).unwrap().into();
    let proof = get_proof::<T>(signer.clone(), recipient.clone(), inner_call_signature);

    let payment_authorisation = get_payment_info::<T>(
        signer.clone(),
        recipient.clone(),
        amount,
        signature.into(),
        token.into(),
    );

    setup_balances::<T>(signer.clone(), amount);

    return (proof, payment_authorisation, signer)
}

benchmarks! {
    charge_fee {
        let recipient: T::AccountId = whitelisted_caller();
        let amount: BalanceOf<T> = 10u32.into();
        #[cfg(test)]
        let token: H160 = crate::mock::AVT_TOKEN_CONTRACT;
        #[cfg(not(test))]
        // Make sure this matched the chainspec value
        let token: H160 = H160(hex!("93ba86eCfDDD9CaAAc29bE83aCE5A3188aC47730"));

        #[cfg(test)]
        let signature: sr25519::Signature = sr25519::Signature::from_slice(&hex!("5c2991619f1c3ae9b3a403b050d1d2dea83e7ae15b43ce632fcbdac32bd8150f0b6790368c0fa51c51ac593f52d48bdfd9f773297a28d5d6a4a13c6c95595989")).unwrap().into();
        #[cfg(not(test))]
        let signature: sr25519::Signature = sr25519::Signature::from_slice(&hex!("bac8dcf41c8603a29d9f50de6b67826ba339306ef5a278bab34a1f334628446844e673fe09043b0e884faa15d2527e8aaa7fb2af6e6c711162678a166534e78a")).unwrap().into();

        let (proof, payment_authorisation, signer) = get_inner_call_proof::<T>(&recipient, amount, token, signature);
    }: {
        AvnProxy::<T>::charge_fee(&proof, payment_authorisation)?;
    }
    verify {
        assert_eq!(T::Currency::free_balance(&recipient), amount.into());
    }

    charge_fee_in_token {
        let recipient: T::AccountId = whitelisted_caller();
        let amount: BalanceOf<T> = 10u32.into();
        #[cfg(test)]
        let token: H160 = crate::mock::NON_AVT_TOKEN_CONTRACT;
        #[cfg(not(test))]
        // Make sure this matched the chainspec value
        let token: H160 = H160(hex!("ea5da4fd16cc61ffc4235874d6ff05216e3e038e"));

        #[cfg(test)]
        let signature: sr25519::Signature = sr25519::Signature::from_slice(&hex!("e4996309bafbf4a57ae54bba2979f6d986da72e49318ac6d05e48fe844d2b614c79803c9dabe462bc94f8f3ca50688f88e8ae79fe56179847576c4f8af64cc87")).unwrap().into();
        #[cfg(not(test))]
        let signature: sr25519::Signature = sr25519::Signature::from_slice(&hex!("6e59c93ff34689af7286aa2d40e940b403c4db76de7bd972837f4e08119aff46f3ebfc9050e110495991dc92e517ada1b622c2f76902f6701ded81878470e984")).unwrap().into();

        let (proof, payment_authorisation, signer) = get_inner_call_proof::<T>(&recipient, amount, token, signature);
        let previous_payment_nonce = <PaymentNonces::<T>>::get(&signer);
    }: {
        AvnProxy::<T>::charge_fee(&proof, payment_authorisation)?;
    }
    verify {
        assert_eq!(previous_payment_nonce + 1, <PaymentNonces::<T>>::get(&signer));
    }
}

impl_benchmark_test_suite!(
    Pallet,
    crate::mock::ExtBuilder::build_default().with_balances().as_externality(),
    crate::mock::TestRuntime,
);
