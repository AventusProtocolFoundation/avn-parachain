//! # Avn transaction payment
// Copyright 2023 Aventus Network Services (UK) Ltd.

//! This is a wrapper pallet for transaction payment that allows the customisation of chain fees
//! based on defined adjustment configuration and a sender.

#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{
    dispatch::{GetDispatchInfo, PostDispatchInfo},
    traits::{Currency},
};
use frame_system::{self as system};

use core::convert::TryInto;
use sp_runtime::{
    traits::{Dispatchable},
};

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::{pallet_prelude::*};

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
    }

    #[pallet::pallet]
    #[pallet::generate_store(pub (super) trait Store)]
    pub struct Pallet<T>(_);

    #[pallet::event]
    #[pallet::generate_deposit(pub fn deposit_event)]
    pub enum Event<T: Config> {
    }
    #[pallet::error]
    pub enum Error<T> {
    }
    #[pallet::call]
    impl<T: Config> Pallet<T> { }
}