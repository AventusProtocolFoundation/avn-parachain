#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::string::String;

use frame_support::traits::IsSubType;
use parity_scale_codec::Encode;
use sp_avn_common::{InnerCallValidator, Proof};
use sp_runtime::traits::{StaticLookup, Verify};
use sp_std::prelude::*;

use super::*;
use crate::Pallet as ParachainStaking;

pub const SIGNED_NOMINATOR_CONTEXT: &'static [u8] = b"authorization for nominate operation";
pub const SIGNED_BOND_EXTRA_CONTEXT: &'static [u8] = b"authorization for bond extra operation";
pub const SIGNED_CANDIDATE_BOND_EXTRA_CONTEXT: &'static [u8] =
    b"authorization for candidate bond extra operation";
pub const SIGNED_UNBOND_CONTEXT: &'static [u8] = b"authorization for unbond operation";
pub const SIGNED_CANDIDATE_UNBOND_CONTEXT: &'static [u8] =
    b"authorization for candidate unbond operation";
pub const SIGNED_NOMINATOR_REMOVE_BOND_CONTEXT: &'static [u8] = b"authorization for nominator remove bond operation";

pub fn get_encoded_call_param<T: Config>(
    call: &<T as Config>::Call,
) -> Option<(&Proof<T::Signature, T::AccountId>, Vec<u8>)> {
    let call = match call.is_sub_type() {
        Some(call) => call,
        None => return None,
    };

    match call {
        Call::signed_nominate { proof, targets, amount } => {
            let sender_nonce = ParachainStaking::<T>::proxy_nonce(&proof.signer);
            let encoded_data = encode_signed_nominate_params::<T>(
                proof.relayer.clone(),
                targets,
                amount,
                sender_nonce,
            );

            return Some((proof, encoded_data))
        },
        Call::signed_bond_extra { proof, extra_amount } => {
            let sender_nonce = ParachainStaking::<T>::proxy_nonce(&proof.signer);
            let encoded_data = encode_signed_bond_extra_params::<T>(
                proof.relayer.clone(),
                extra_amount,
                sender_nonce,
            );

            return Some((proof, encoded_data))
        },
        Call::signed_candidate_bond_extra { proof, extra_amount } => {
            let sender_nonce = ParachainStaking::<T>::proxy_nonce(&proof.signer);
            let encoded_data = encode_signed_candidate_bond_extra_params::<T>(
                proof.relayer.clone(),
                extra_amount,
                sender_nonce,
            );

            return Some((proof, encoded_data))
        },
        Call::signed_schedule_candidate_unbond { proof, less } => {
            let sender_nonce = ParachainStaking::<T>::proxy_nonce(&proof.signer);
            let encoded_data = encode_signed_schedule_candidate_unbond_params::<T>(
                proof.relayer.clone(),
                less,
                sender_nonce,
            );

            return Some((proof, encoded_data))
        },
        Call::signed_schedule_nominator_unbond { proof, less } => {
            let sender_nonce = ParachainStaking::<T>::proxy_nonce(&proof.signer);
            let encoded_data = encode_signed_schedule_nominator_unbond_params::<T>(
                proof.relayer.clone(),
                less,
                sender_nonce,
            );

            return Some((proof, encoded_data))
        },
        Call::signed_schedule_revoke_nomination { proof, collator } => {
            let sender_nonce = ParachainStaking::<T>::proxy_nonce(&proof.signer);
            let encoded_data = encode_signed_schedule_revoke_nomination_params::<T>(
                proof.relayer.clone(),
                collator,
                sender_nonce,
            );

            return Some((proof, encoded_data))
        },
        _ => return None,
    }
}

pub fn encode_signed_nominate_params<T: Config>(
    relayer: T::AccountId,
    targets: &Vec<<T::Lookup as StaticLookup>::Source>,
    amount: &BalanceOf<T>,
    sender_nonce: u64,
) -> Vec<u8> {
    return (SIGNED_NOMINATOR_CONTEXT, relayer, targets, amount, sender_nonce).encode()
}

pub fn encode_signed_bond_extra_params<T: Config>(
    relayer: T::AccountId,
    extra_amount: &BalanceOf<T>,
    sender_nonce: u64,
) -> Vec<u8> {
    return (SIGNED_BOND_EXTRA_CONTEXT, relayer, extra_amount, sender_nonce).encode()
}

pub fn encode_signed_candidate_bond_extra_params<T: Config>(
    relayer: T::AccountId,
    extra_amount: &BalanceOf<T>,
    sender_nonce: u64,
) -> Vec<u8> {
    return (SIGNED_CANDIDATE_BOND_EXTRA_CONTEXT, relayer, extra_amount, sender_nonce).encode()
}

pub fn encode_signed_schedule_nominator_unbond_params<T: Config>(
    relayer: T::AccountId,
    value: &BalanceOf<T>,
    sender_nonce: u64,
) -> Vec<u8> {
    return (SIGNED_UNBOND_CONTEXT, relayer, value, sender_nonce).encode()
}

pub fn encode_signed_schedule_candidate_unbond_params<T: Config>(
    relayer: T::AccountId,
    value: &BalanceOf<T>,
    sender_nonce: u64,
) -> Vec<u8> {
    return (SIGNED_CANDIDATE_UNBOND_CONTEXT, relayer, value, sender_nonce).encode()
}

pub fn encode_signed_schedule_revoke_nomination_params<T: Config>(
    relayer: T::AccountId,
    collator: &T::AccountId,
    sender_nonce: u64,
) -> Vec<u8> {
    return (SIGNED_NOMINATOR_REMOVE_BOND_CONTEXT, relayer, collator, sender_nonce).encode()
}

pub fn verify_signature<T: Config>(
    proof: &Proof<T::Signature, T::AccountId>,
    signed_payload: &[u8],
) -> Result<(), Error<T>> {
    match proof.signature.verify(signed_payload, &proof.signer) {
        true => Ok(()),
        false => Err(<Error<T>>::UnauthorizedProxyTransaction.into()),
    }
}

impl<T: Config> InnerCallValidator for ParachainStaking<T> {
    type Call = <T as Config>::Call;

    fn signature_is_valid(call: &Box<Self::Call>) -> bool {
        if let Some((proof, signed_payload)) = get_encoded_call_param::<T>(call) {
            return verify_signature::<T>(&proof, &signed_payload.as_slice()).is_ok()
        }

        return false
    }
}
