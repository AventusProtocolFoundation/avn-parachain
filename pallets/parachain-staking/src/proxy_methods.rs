#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::{string::String};

use parity_scale_codec::{Encode};
use sp_std::{prelude::*};
use sp_avn_common::{InnerCallValidator, Proof};
use sp_runtime::traits::{StaticLookup, Verify};
use frame_support::traits::IsSubType;

use super::{*};
use crate::{Pallet as ParachainStaking};

pub const SIGNED_NOMINATOR_CONTEXT: &'static [u8] = b"authorization for nominate operation";

pub fn get_encoded_call_param<T: Config>(call: &<T as Config>::Call) -> Option<(&Proof<T::Signature, T::AccountId>, Vec<u8>)> {
    let call = match call.is_sub_type() {
        Some(call) => call,
        None => return None,
    };

    match call {
        Call::signed_nominate { proof, targets } => {
            let sender_nonce = ParachainStaking::<T>::proxy_nonce(&proof.signer);
            let encoded_data = encode_signed_nominate_params::<T>(proof, targets, sender_nonce);

            return Some((proof, encoded_data));
        },
        _ => return None
    }
}

pub fn encode_signed_nominate_params<T: Config>(
    proof: &Proof<T::Signature, T::AccountId>,
    targets: &Vec<<T::Lookup as StaticLookup>::Source>,
    sender_nonce: u64) -> Vec<u8>
{
    return (SIGNED_NOMINATOR_CONTEXT, proof.relayer.clone(), targets, sender_nonce).encode();
}

pub fn verify_signature<T: Config>(proof: &Proof<T::Signature, T::AccountId>, signed_payload: &[u8]) -> Result<(), Error<T>> {
    match proof.signature.verify(signed_payload, &proof.signer) {
        true => Ok(()),
        false => Err(<Error<T>>::UnauthorizedProxyTransaction.into()),
    }
}

impl<T: Config> InnerCallValidator for ParachainStaking<T> {
    type Call = <T as Config>::Call;

    fn signature_is_valid(call: &Box<Self::Call>) -> bool {
        if let Some((proof, signed_payload)) = get_encoded_call_param::<T>(call) {
            return verify_signature::<T>(&proof, &signed_payload.as_slice()).is_ok();
        }

        return false;
    }
}