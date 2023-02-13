#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::string::String;

use codec::Encode;
use sp_std::prelude::*;

use super::*;
use crate::Pallet as ValidatorsManager;

pub const SIGNED_BOND_CONTEXT: &'static [u8] = b"authorization for bond operation";
pub const SIGNED_NOMINATOR_CONTEXT: &'static [u8] = b"authorization for nominate operation";
pub const SIGNED_REBOND_CONTEXT: &'static [u8] = b"authorization for rebond operation";
pub const SIGNED_PAYOUT_STAKERS_CONTEXT: &'static [u8] =
    b"authorization for signed payout stakers operation";
pub const SIGNED_SET_CONTROLLER_CONTEXT: &'static [u8] =
    b"authorization for set controller operation";
pub const SIGNED_SET_PAYEE_CONTEXT: &'static [u8] = b"authorization for set payee operation";
pub const SIGNED_WITHDRAW_UNBONDED_CONTEXT: &'static [u8] =
    b"authorization for withdraw unbonded operation";
pub const SIGNED_UNBOND_CONTEXT: &'static [u8] = b"authorization for unbond operation";
pub const SIGNED_BOND_EXTRA_CONTEXT: &'static [u8] = b"authorization for bond extra operation";

pub fn get_encoded_call_param<T: Config>(
    call: &<T as Config>::Call,
) -> Option<(&Proof<T::Signature, T::AccountId>, Vec<u8>)> {
    let call = match call.is_sub_type() {
        Some(call) => call,
        None => return None,
    };

    match call {
        Call::signed_bond(proof, controller, amount, payee) => {
            let sender_nonce = <ValidatorsManager<T> as Store>::ProxyNonces::get(&proof.signer);
            let encoded_data =
                encode_signed_bond_params::<T>(proof, controller, amount, payee, sender_nonce);

            return Some((proof, encoded_data));
        },
        Call::signed_nominate(proof, targets) => {
            let sender_nonce = <ValidatorsManager<T> as Store>::ProxyNonces::get(&proof.signer);
            let encoded_data = encode_signed_nominate_params::<T>(proof, targets, sender_nonce);

            return Some((proof, encoded_data));
        },
        Call::signed_rebond(proof, value) => {
            let sender_nonce = <ValidatorsManager<T> as Store>::ProxyNonces::get(&proof.signer);
            let encoded_data = encode_signed_rebond_params::<T>(proof, value, sender_nonce);

            return Some((proof, encoded_data));
        },
        Call::signed_payout_stakers(proof, era) => {
            let sender_nonce = <ValidatorsManager<T> as Store>::ProxyNonces::get(&proof.signer);
            let encoded_data = encode_signed_payout_stakers_params::<T>(proof, era, sender_nonce);

            return Some((proof, encoded_data));
        },
        Call::signed_set_controller(proof, controller) => {
            let sender_nonce = <ValidatorsManager<T> as Store>::ProxyNonces::get(&proof.signer);
            let encoded_data =
                encode_signed_set_controller_params::<T>(proof, controller, sender_nonce);

            return Some((proof, encoded_data));
        },
        Call::signed_set_payee(proof, payee) => {
            let sender_nonce = <ValidatorsManager<T> as Store>::ProxyNonces::get(&proof.signer);
            let encoded_data = encode_signed_set_payee_params::<T>(proof, payee, sender_nonce);

            return Some((proof, encoded_data));
        },
        Call::signed_withdraw_unbonded(proof, num_slashing_spans) => {
            let sender_nonce = <ValidatorsManager<T> as Store>::ProxyNonces::get(&proof.signer);
            let encoded_data = encode_signed_withdraw_unbonded_params::<T>(
                proof,
                num_slashing_spans,
                sender_nonce,
            );

            return Some((proof, encoded_data));
        },
        Call::signed_unbond(proof, value) => {
            let sender_nonce = <ValidatorsManager<T> as Store>::ProxyNonces::get(&proof.signer);
            let encoded_data = encode_signed_unbond_params::<T>(proof, value, sender_nonce);

            return Some((proof, encoded_data));
        },
        Call::signed_bond_extra(proof, max_additional) => {
            let sender_nonce = <ValidatorsManager<T> as Store>::ProxyNonces::get(&proof.signer);
            let encoded_data =
                encode_signed_bond_extra_params::<T>(proof, max_additional, sender_nonce);

            return Some((proof, encoded_data));
        },
        _ => return None,
    }
}

pub fn encode_signed_bond_params<T: Config>(
    proof: &Proof<T::Signature, T::AccountId>,
    controller: &<T::Lookup as StaticLookup>::Source,
    value: &BalanceOf<T>,
    payee: &RewardDestination<T::AccountId>,
    sender_nonce: u64,
) -> Vec<u8> {
    return (SIGNED_BOND_CONTEXT, proof.relayer.clone(), controller, value, payee, sender_nonce)
        .encode();
}

pub fn encode_signed_nominate_params<T: Config>(
    proof: &Proof<T::Signature, T::AccountId>,
    targets: &Vec<<T::Lookup as StaticLookup>::Source>,
    sender_nonce: u64,
) -> Vec<u8> {
    return (SIGNED_NOMINATOR_CONTEXT, proof.relayer.clone(), targets, sender_nonce).encode();
}

pub fn encode_signed_rebond_params<T: Config>(
    proof: &Proof<T::Signature, T::AccountId>,
    value: &BalanceOf<T>,
    sender_nonce: u64,
) -> Vec<u8> {
    return (SIGNED_REBOND_CONTEXT, proof.relayer.clone(), value, sender_nonce).encode();
}

pub fn encode_signed_payout_stakers_params<T: Config>(
    proof: &Proof<T::Signature, T::AccountId>,
    era: &EraIndex,
    sender_nonce: u64,
) -> Vec<u8> {
    return (SIGNED_PAYOUT_STAKERS_CONTEXT, proof.relayer.clone(), era, sender_nonce).encode();
}

pub fn encode_signed_set_controller_params<T: Config>(
    proof: &Proof<T::Signature, T::AccountId>,
    controller: &<T::Lookup as StaticLookup>::Source,
    sender_nonce: u64,
) -> Vec<u8> {
    return (SIGNED_SET_CONTROLLER_CONTEXT, proof.relayer.clone(), controller, sender_nonce)
        .encode();
}

pub fn encode_signed_set_payee_params<T: Config>(
    proof: &Proof<T::Signature, T::AccountId>,
    payee: &RewardDestination<T::AccountId>,
    sender_nonce: u64,
) -> Vec<u8> {
    return (SIGNED_SET_PAYEE_CONTEXT, proof.relayer.clone(), payee, sender_nonce).encode();
}

pub fn encode_signed_withdraw_unbonded_params<T: Config>(
    proof: &Proof<T::Signature, T::AccountId>,
    num_slashing_spans: &u32,
    sender_nonce: u64,
) -> Vec<u8> {
    return (
        SIGNED_WITHDRAW_UNBONDED_CONTEXT,
        proof.relayer.clone(),
        num_slashing_spans,
        sender_nonce,
    )
        .encode();
}

pub fn encode_signed_unbond_params<T: Config>(
    proof: &Proof<T::Signature, T::AccountId>,
    value: &BalanceOf<T>,
    sender_nonce: u64,
) -> Vec<u8> {
    return (SIGNED_UNBOND_CONTEXT, proof.relayer.clone(), value, sender_nonce).encode();
}

pub fn encode_signed_bond_extra_params<T: Config>(
    proof: &Proof<T::Signature, T::AccountId>,
    max_additional: &BalanceOf<T>,
    sender_nonce: u64,
) -> Vec<u8> {
    return (SIGNED_BOND_EXTRA_CONTEXT, proof.relayer.clone(), max_additional, sender_nonce)
        .encode();
}
