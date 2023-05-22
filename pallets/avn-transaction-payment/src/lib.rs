//! # Avn transaction payment
// Copyright 2023 Aventus Network Services (UK) Ltd.

//! This is a wrapper pallet for  transaction payment that allows the customisation of chain fees based
//! on the business logic of this pallet
//! assumption about where the transaction is coming from.

#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{
    dispatch::{DispatchResult,GetDispatchInfo, PostDispatchInfo},
    pallet_prelude::ValueQuery,
    traits::{Currency, Imbalance, OnUnbalanced},
    unsigned::TransactionValidityError,
};
use frame_system::{self as system};

use core::convert::{TryInto};
pub use pallet::*;
use sp_runtime::{
    traits::{ DispatchInfoOf, PostDispatchInfoOf, Saturating, Dispatchable },
	transaction_validity::InvalidTransaction, Perbill
};

use pallet_transaction_payment::{OnChargeTransaction, CurrencyAdapter};
use sp_std::{marker::PhantomData, prelude::*};

pub mod fee_config;
use fee_config::*;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::{pallet_prelude::*, Blake2_128Concat};
    use frame_system::pallet_prelude::*;

    #[pallet::config]
    pub trait Config: frame_system::Config + pallet_transaction_payment::Config {
        /// The overarching event type.
        type RuntimeEvent: From<Event<Self>>
            + Into<<Self as system::Config>::RuntimeEvent>
            + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// The overarching call type
        type RuntimeCall: Parameter
            + Dispatchable<RuntimeOrigin = Self::RuntimeOrigin, PostInfo = PostDispatchInfo>
            + GetDispatchInfo
            + From<frame_system::Call<Self>>;

        /// Currency type for processing fee payment
        type Currency: Currency<Self::AccountId>;

        type WeightInfo: WeightInfo;
    }

    #[pallet::pallet]
    #[pallet::generate_store(pub (super) trait Store)]
    pub struct Pallet<T>(_);

    #[pallet::event]
    #[pallet::generate_deposit(pub fn deposit_event)]
    pub enum Event<T: Config> {
		/// A transaction fee has been adjusted by `adjustment`, for `who`
		AdjustedTransactionFeePaid { who: T::AccountId, adjustment: BalanceOf<T>},
    }

    #[pallet::error]
    pub enum Error<T> {
        InvalidFeeType
    }

    #[pallet::storage]
    #[pallet::getter(fn known_senders)]
    /// A map of known senders
    pub type KnownSenders<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, bool, ValueQuery>;

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(0)]
        pub fn set_known_sender(origin: OriginFor<T>, known_sender: T::AccountId) -> DispatchResult {
            frame_system::ensure_root(origin)?;
            <KnownSenders<T>>::insert(known_sender, true);
            Ok(())
        }
    }
}

pub(crate) type BalanceOf<T> =
    <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

type NegativeImbalanceOf<C, T> =
    <C as Currency<<T as frame_system::Config>::AccountId>>::NegativeImbalance;

impl<T: Config> Pallet<T> {
    pub fn is_known_sender(address: &<T as frame_system::Config>::AccountId) -> bool {
        return <KnownSenders<T>>::contains_key(address)
    }
}

/// Implements the transaction payment for a pallet implementing the `Currency`
/// trait (eg. the pallet_balances) using an unbalance handler (implementing
/// `OnUnbalanced`).
///
/// The unbalance handler is given 2 unbalanceds in [`OnUnbalanced::on_unbalanceds`]: fee and
/// then tip.
pub struct AvnCurrencyAdapter<C, OU>(PhantomData<(C, OU)>);

/// Default implementation for a Currency and an OnUnbalanced handler.
///
/// The unbalance handler is given 2 unbalanceds in [`OnUnbalanced::on_unbalanceds`]: fee and
/// then tip.
impl<T, C, OU> OnChargeTransaction<T> for AvnCurrencyAdapter<C, OU>
where
	T: Config + pallet::Config<Currency = C>,
	C: Currency<<T as frame_system::Config>::AccountId>,
	C::PositiveImbalance: Imbalance<
		<C as Currency<<T as frame_system::Config>::AccountId>>::Balance,
		Opposite = C::NegativeImbalance,
	>,
	C::NegativeImbalance: Imbalance<
		<C as Currency<<T as frame_system::Config>::AccountId>>::Balance,
		Opposite = C::PositiveImbalance,
	>,
	OU: OnUnbalanced<NegativeImbalanceOf<C, T>>,
{
	type LiquidityInfo = Option<NegativeImbalanceOf<C, T>>;
	type Balance = <C as Currency<<T as frame_system::Config>::AccountId>>::Balance;

	/// Withdraw the predicted fee from the transaction origin.
	///
	/// Note: The `fee` already includes the `tip`.
	fn withdraw_fee(
		who: &<T as frame_system::Config>::AccountId,
		_call: &<T as frame_system::Config>::RuntimeCall,
		_info: &DispatchInfoOf<<T as frame_system::Config>::RuntimeCall>,
		fee: Self::Balance,
		tip: Self::Balance,
	) -> Result<Self::LiquidityInfo, TransactionValidityError> {
        return <CurrencyAdapter::<C, OU> as OnChargeTransaction<T>>::withdraw_fee(who, _call, _info, fee, tip);
	}

	/// Hand the fee and the tip over to the `[OnUnbalanced]` implementation.
	/// Since the predicted fee might have been too high, parts of the fee may
	/// be refunded.
	///
	/// Note: The `corrected_fee` already includes the `tip`.
	fn correct_and_deposit_fee(
		who: &<T as frame_system::Config>::AccountId,
		_dispatch_info: &DispatchInfoOf<<T as frame_system::Config>::RuntimeCall>,
		_post_info: &PostDispatchInfoOf<<T as frame_system::Config>::RuntimeCall>,
		corrected_fee: Self::Balance,
		tip: Self::Balance,
		already_withdrawn: Self::LiquidityInfo,
	) -> Result<(), TransactionValidityError> {
		if let Some(paid) = already_withdrawn {


            // Calculate how much refund we should return
            let mut discounted_corrected_fee: Self::Balance = corrected_fee;

            let is_known_sender = Pallet::<T>::is_known_sender(who);
            if is_known_sender {
                let discount = Perbill::from_percent(50);
                discounted_corrected_fee = discount * corrected_fee;
            }

            let refund_amount = paid.peek().saturating_sub(discounted_corrected_fee);


			// refund to the the account that paid the fees. If this fails, the
			// account might have dropped below the existential balance. In
			// that case we don't refund anything.
			let refund_imbalance = C::deposit_into_existing(who, refund_amount)
				.unwrap_or_else(|_| C::PositiveImbalance::zero());
			// merge the imbalance caused by paying the fees and refunding parts of it again.
			let adjusted_paid = paid
				.offset(refund_imbalance)
				.same()
				.map_err(|_| TransactionValidityError::Invalid(InvalidTransaction::Payment))?;
			// Call someone else to handle the imbalance (fee and tip separately)
			let (tip, fee) = adjusted_paid.split(tip);
			OU::on_unbalanceds(Some(fee).into_iter().chain(Some(tip)));

            if is_known_sender {
                Pallet::<T>::deposit_event(Event::<T>::AdjustedTransactionFeePaid { who: who.clone(), adjustment: discounted_corrected_fee});
            }
		}
		Ok(())
	}
}

// #[cfg(test)]
// #[path = "tests/mock.rs"]
// mod mock;

// #[cfg(test)]
// #[path = "tests/proxy_tests_no_fees.rs"]
// pub mod proxy_tests_no_fees;

// #[cfg(test)]
// #[path = "tests/proxy_tests_with_fees.rs"]
// pub mod proxy_tests_with_fees;

pub mod default_weights;
pub use default_weights::WeightInfo;

// mod benchmarking;
