//! # Avn transaction payment
// Copyright 2023 Aventus Network Services (UK) Ltd.

//! This is a wrapper pallet for transaction payment that allows the customisation of chain fees
//! based on defined adjustment configuration and a sender.

#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{
    dispatch::{GetDispatchInfo, PostDispatchInfo},
    traits::Currency,
};
use frame_system::{self as system};

use core::convert::TryInto;
pub use pallet::*;
use sp_runtime::traits::Dispatchable;

pub mod fee_adjustment_config;
use fee_adjustment_config::{
    AdjustmentType::{TimeBased, TransactionBased},
    *,
};

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
    }

    #[pallet::pallet]
    #[pallet::generate_store(pub (super) trait Store)]
    pub struct Pallet<T>(_);

    #[pallet::event]
    #[pallet::generate_deposit(pub fn deposit_event)]
    pub enum Event<T: Config> {
        /// A new known sender has been added
        KnownSenderAdded { known_sender: T::AccountId, adjustment: FeeAdjustmentConfig<T> },
        /// Adjustments have been updated for an existing known sender
        KnownSenderUpdated { known_sender: T::AccountId, adjustment: FeeAdjustmentConfig<T> },
    }

    #[pallet::error]
    pub enum Error<T> {
        InvalidFeeConfig,
        InvalidFeeType,
        KnownSenderMustMatchAccount,
    }

    #[pallet::storage]
    #[pallet::getter(fn known_senders)]
    /// A map of known senders
    pub type KnownSenders<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, FeeAdjustmentConfig<T>, ValueQuery>;

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(0)]
        pub fn set_known_sender(
            origin: OriginFor<T>,
            known_sender: T::AccountId,
            config: AdjustmentInput<T>,
        ) -> DispatchResult {
            frame_system::ensure_root(origin)?;

            let mut fee_adjustment_config: FeeAdjustmentConfig<T> = Default::default();
            if config.adjustment_type != AdjustmentType::None {
                match config.adjustment_type {
                    TimeBased(b) => {
                        fee_adjustment_config = FeeAdjustmentConfig::TimeBased(
                            TimeBasedConfig::new(config.fee_type, b.duration),
                        );
                    },
                    TransactionBased(i) => {
                        fee_adjustment_config =
                            FeeAdjustmentConfig::TransactionBased(TransactionBasedConfig::new(
                                config.fee_type,
                                known_sender.clone(),
                                i.number_of_transactions,
                            ));
                    },
                    _ => {},
                }
            } else {
                match config.fee_type {
                    FeeType::FixedFee(f) => {
                        fee_adjustment_config = FeeAdjustmentConfig::FixedFee(f);
                    },
                    FeeType::PercentageFee(p) => {
                        fee_adjustment_config = FeeAdjustmentConfig::PercentageFee(p);
                    },
                    _ => {},
                }
            }

            ensure!(fee_adjustment_config.is_valid() == true, Error::<T>::InvalidFeeConfig);

            let sender_exists = <KnownSenders<T>>::contains_key(&known_sender);
            <KnownSenders<T>>::insert(&known_sender, &fee_adjustment_config);

            if !sender_exists {
                Self::deposit_event(Event::<T>::KnownSenderAdded {
                    known_sender,
                    adjustment: fee_adjustment_config,
                });
            } else {
                Self::deposit_event(Event::<T>::KnownSenderUpdated {
                    known_sender,
                    adjustment: fee_adjustment_config,
                });
            }

            Ok(())
        }
    }
}

pub(crate) type BalanceOf<T> =
    <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;
