//! # Avn proxy pallet
// Copyright 2022 Aventus Network Services (UK) Ltd.

//! The avnProxy pallet is responsible for proxying transactions to a list of whitelisted pallets.
//! The target pallets are responsible for validating the transaction and should not make any
//! assumption about where the transaction is coming from.

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
use frame_support::{
    dispatch::{DispatchResult, DispatchResultWithPostInfo, GetDispatchInfo, PostDispatchInfo},
    ensure,
    pallet_prelude::ValueQuery,
    traits::{Currency, ExistenceRequirement, IsSubType},
};
use frame_system::{self as system, ensure_signed};
use sp_avn_common::{InnerCallValidator, Proof, CLOSE_BYTES_TAG, OPEN_BYTES_TAG};

use core::convert::TryInto;
pub use pallet::*;
use sp_runtime::{
    scale_info::TypeInfo,
    traits::{Dispatchable, Hash, IdentifyAccount, Member, Verify},
};
use sp_std::prelude::*;

pub const PAYMENT_AUTH_CONTEXT: &'static [u8] = b"authorization for proxy payment";

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::{
        pallet_prelude::{DispatchResult, *},
        Blake2_128Concat,
    };
    use frame_system::pallet_prelude::*;

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// The overarching event type.
        type RuntimeEvent: From<Event<Self>>
            + Into<<Self as system::Config>::RuntimeEvent>
            + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// The overarching call type
        type RuntimeCall: Parameter
            + Dispatchable<RuntimeOrigin = Self::RuntimeOrigin, PostInfo = PostDispatchInfo>
            + GetDispatchInfo
            + From<frame_system::Call<Self>>
            + IsSubType<Call<Self>>;

        /// Currency type for processing fee payment
        type Currency: Currency<Self::AccountId>;

        /// A type that can be used to verify signatures
        type Public: IdentifyAccount<AccountId = Self::AccountId>;

        /// The signature type used by accounts/transactions.
        type Signature: Verify<Signer = Self::Public>
            + Member
            + Decode
            + Encode
            + From<sp_core::sr25519::Signature>
            + TypeInfo;

        type ProxyConfig: Parameter
            + Member
            + Ord
            + PartialOrd
            + Default
            + ProvableProxy<<Self as Config>::RuntimeCall, Self::Signature, Self::AccountId>
            + InnerCallValidator<Call = <Self as Config>::RuntimeCall>;

        type WeightInfo: WeightInfo;
    }

    #[pallet::pallet]
    #[pallet::generate_store(pub (super) trait Store)]
    pub struct Pallet<T>(_);

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        CallDispatched { relayer: T::AccountId, hash: T::Hash },
        InnerCallFailed { relayer: T::AccountId, hash: T::Hash, dispatch_error: DispatchError },
    }

    #[pallet::error]
    pub enum Error<T> {
        TransactionNotSupported,
        UnauthorizedFee,
        UnauthorizedProxyTransaction,
    }

    #[pallet::storage]
    #[pallet::getter(fn payment_nonces)]
    /// An account nonce that represents the number of payments from this account
    /// It is shared for all proxy transactions performed by that account
    pub type PaymentNonces<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, u64, ValueQuery>;

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(<T as pallet::Config>::WeightInfo::charge_fee().saturating_add(call.get_dispatch_info().weight).saturating_add(Weight::from_ref_time(50_000)))]
        pub fn proxy(
            origin: OriginFor<T>,
            call: Box<<T as Config>::RuntimeCall>,
            payment_info: Option<Box<PaymentInfo<T::AccountId, BalanceOf<T>, T::Signature>>>,
        ) -> DispatchResultWithPostInfo {
            let relayer = ensure_signed(origin)?;
            let mut final_weight =
                call.get_dispatch_info().weight.saturating_add(Weight::from_ref_time(50_000));

            let proof = <T as Config>::ProxyConfig::get_proof(&call)
                .ok_or(Error::<T>::TransactionNotSupported)?;
            ensure!(relayer == proof.relayer, Error::<T>::UnauthorizedProxyTransaction);

            if let Some(payment_info) = payment_info {
                final_weight = T::WeightInfo::charge_fee()
                    .saturating_add(call.get_dispatch_info().weight)
                    .saturating_add(Weight::from_ref_time(50_000));
                // If the inner call signature does not validate, exit without charging the sender a
                // fee
                Self::validate_inner_call_signature(&call)?;
                Self::charge_fee(&proof, *payment_info)?;
            }

            let call_hash: T::Hash = T::Hashing::hash_of(&call);
            let sender: T::RuntimeOrigin =
                frame_system::RawOrigin::Signed(proof.signer.clone()).into();

            let dispatch_result = call.dispatch(sender).map(|_| ()).map_err(|e| e.error);
            match dispatch_result {
                Ok(_) => {
                    Self::deposit_event(Event::<T>::CallDispatched { relayer, hash: call_hash });
                },
                Err(dispatch_error) => {
                    Self::deposit_event(Event::<T>::InnerCallFailed {
                        relayer,
                        hash: call_hash,
                        dispatch_error,
                    });
                },
            }

            Ok(Some(final_weight).into())
        }
    }
}

pub(crate) type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

impl<T: Config> Pallet<T> {
    fn validate_inner_call_signature(call: &Box<<T as Config>::RuntimeCall>) -> DispatchResult {
        let inner_call_sig_valid = <T as Config>::ProxyConfig::signature_is_valid(call);
        if inner_call_sig_valid == false {
            return Err(Error::<T>::UnauthorizedProxyTransaction)?
        }

            Ok(())
        }

    fn verify_payment_authorisation_signature(
        proof: &Proof<T::Signature, T::AccountId>,
        payment_info: &PaymentInfo<T::AccountId, BalanceOf<T>, T::Signature>,
        payment_nonce: u64,
    ) -> Result<(), Error<T>> {
        let encoded_payload = (
            PAYMENT_AUTH_CONTEXT,
            &proof,
            &payment_info.recipient,
            &payment_info.amount,
            payment_nonce,
        )
            .encode();

        // TODO: centralise wrapped payload signature verification logic in primitives if
        // possible.
        let wrapped_encoded_payload: Vec<u8> =
            [OPEN_BYTES_TAG, encoded_payload.as_slice(), CLOSE_BYTES_TAG].concat();
        match payment_info.signature.verify(&*wrapped_encoded_payload, &payment_info.payer) {
            true => Ok(()),
            false => match payment_info
                .signature
                .verify(encoded_payload.as_slice(), &payment_info.payer)
            {
                true => Ok(()),
                false => Err(<Error<T>>::UnauthorizedFee.into()),
            },
        }
    }

    pub(crate) fn charge_fee(
        proof: &Proof<T::Signature, T::AccountId>,
        payment_info: PaymentInfo<T::AccountId, BalanceOf<T>, T::Signature>,
    ) -> DispatchResult {
        let payment_nonce = Self::payment_nonces(&payment_info.payer);
        ensure!(
            Self::verify_payment_authorisation_signature(proof, &payment_info, payment_nonce)
                .is_ok(),
            Error::<T>::UnauthorizedFee
        );

        T::Currency::transfer(
            &payment_info.payer,
            &payment_info.recipient,
            payment_info.amount,
            ExistenceRequirement::KeepAlive,
        )?;

        // Only increment the nonce if the charge goes through
        <PaymentNonces<T>>::mutate(&payment_info.payer, |n| *n += 1);

        Ok(())
    }
}

pub trait ProvableProxy<Call, Signature: scale_info::TypeInfo, AccountId>:
    Sized + Send + Sync
{
    fn get_proof(call: &Call) -> Option<Proof<Signature, AccountId>>;
}

#[derive(Encode, Decode, Copy, Clone, PartialEq, Eq, Default, Debug, TypeInfo)]
pub struct PaymentInfo<AccountId, Balance, Signature: TypeInfo> {
    pub payer: AccountId,
    pub recipient: AccountId,
    pub amount: Balance,
    pub signature: Signature,
}

#[cfg(test)]
#[path = "tests/mock.rs"]
mod mock;

#[cfg(test)]
#[path = "tests/proxy_tests_no_fees.rs"]
pub mod proxy_tests_no_fees;

#[cfg(test)]
#[path = "tests/proxy_tests_with_fees.rs"]
pub mod proxy_tests_with_fees;

pub mod default_weights;
pub use default_weights::WeightInfo;

mod benchmarking;
