pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    use frame_support::{pallet_prelude::*, weights::WeightMeter};
    use frame_system::{
        offchain::{SendTransactionTypes, SubmitTransaction},
        pallet_prelude::*,
    };
    use log;
    use pallet_avn::{self as avn, Error as avn_error};
    use pallet_timestamp as timestamp;
    use serde_json::Value;
    use sp_avn_common::event_types::Validator;
    use sp_core::U256;
    use sp_runtime::{traits::Saturating, DispatchError, RuntimeAppPublic};

    const PALLET_NAME: &'static [u8] = b"AvnOracle";
    pub const PRICE_SUBMISSION_CONTEXT: &'static [u8] = b"update_price_signing_context";
    pub const CLEAR_CONSENSUS_SUBMISSION_CONTEXT: &'static [u8] =
        b"clear_consensus_signing_context";
    const BATCH_PER_STORAGE: usize = 6;
    pub const MAX_DELETE_ATTEMPTS: u32 = 5;
    const MAX_SYMBOL_LENGTH: usize = 4;
    const MAX_CURRENCIES: usize = 10;

    pub type AVN<T> = avn::Pallet<T>;

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    #[pallet::getter(fn nonce)]
    pub type Nonce<T> = StorageValue<_, u32, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn price_submission_timestamps)]
    pub type PriceSubmissionTimestamps<T: Config> =
        StorageMap<_, Blake2_128Concat, u32, (u64, u64), OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn price_reporters)]
    pub type PriceReporters<T: Config> =
        StorageDoubleMap<_, Blake2_128Concat, u32, Blake2_128Concat, T::AccountId, (), ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn last_price_submission)]
    pub type LastPriceSubmission<T: Config> = StorageValue<_, BlockNumberFor<T>, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn last_cleared_nonces)]
    pub type LastClearedNonces<T: Config> = StorageValue<_, (u32, u32), OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn processed_nonces)]
    pub type ProcessedNonces<T: Config> = StorageValue<_, u32, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn reported_rates)]
    pub type ReportedRates<T: Config> =
        StorageDoubleMap<_, Blake2_128Concat, u32, Blake2_128Concat, Rates, u32, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn native_toke_rate_by_currency)]
    pub type NativeTokenRateByCurrency<T: Config> =
        StorageMap<_, Blake2_128Concat, CurrencySymbol, U256, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn currency_symbols)]
    pub type CurrencySymbols<T: Config> =
        StorageMap<_, Blake2_128Concat, CurrencySymbol, (), OptionQuery>;

    #[pallet::config]
    pub trait Config:
        SendTransactionTypes<Call<Self>>
        + frame_system::Config
        + pallet_avn::Config
        + timestamp::Config
    {
        /// The overarching runtime event type.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        /// A type representing the weights required by the dispatchables of this pallet.
        type WeightInfo;

        /// How often fiat rates should be refreshed, in blocks
        #[pallet::constant]
        type PriceRefreshRangeInBlocks: Get<u32>;

        /// How often fiat rates should be refreshed, in blocks
        #[pallet::constant]
        type ConsensusGracePeriod: Get<u32>;

        /// Maximum number of currencies
        #[pallet::constant]
        type MaxCurrencies: Get<u32>;
    }

    #[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
    pub struct CurrencySymbol(pub Vec<u8>);

    impl MaxEncodedLen for CurrencySymbol {
        fn max_encoded_len() -> usize {
            MAX_SYMBOL_LENGTH
        }
    }

    #[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
    pub struct Rates(pub Vec<(CurrencySymbol, U256)>);

    impl MaxEncodedLen for Rates {
        fn max_encoded_len() -> usize {
            MAX_CURRENCIES
        }
    }

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        PriceUpdated { rates: Rates, nonce: u32 },
        ConsensusCleared { period: u32 },
        CurrencyRegistered { symbol: Vec<u8> },
        CurrencyRemoved { symbol: Vec<u8> },
    }

    #[pallet::error]
    pub enum Error<T> {
        SubmitterNotAValidator,
        ErrorSigning,
        ErrorSubmittingTransaction,
        ErrorFetchingPrice,
        ValidatorAlreadySubmitted,
        PriceMustBeGreaterThanZero,
        InvalidRateFormat,
        MissingPriceTimestamps,
        InvalidCurrency,
        TooManyCurrencies,
        GracePeriodNotPassed,
        CurrencyNotFound,
    }

    #[pallet::storage]
    pub type Something<T> = StorageValue<_, u32>;

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(Weight::default())]
        pub fn submit_price(
            origin: OriginFor<T>,
            rates: Rates,
            submitter: Validator<T::AuthorityId, T::AccountId>,
            _signature: <<T as avn::Config>::AuthorityId as RuntimeAppPublic>::Signature,
        ) -> DispatchResultWithPostInfo {
            ensure_none(origin)?;
            ensure!(
                AVN::<T>::is_validator(&submitter.account_id),
                Error::<T>::SubmitterNotAValidator
            );

            let nonce = Nonce::<T>::get();
            ensure!(
                !PriceReporters::<T>::contains_key(nonce, &submitter.account_id),
                Error::<T>::ValidatorAlreadySubmitted
            );
            PriceReporters::<T>::insert(nonce, &submitter.account_id, ());

            let count = ReportedRates::<T>::mutate(nonce, &rates, |count| {
                *count = count.saturating_add(1);
                *count
            });

            if count > AVN::<T>::quorum() {
                log::info!("🎁 Quorum reached: {}, proceeding to publish rates", count);
                Self::deposit_event(Event::<T>::PriceUpdated { rates: rates.clone(), nonce });

                for (symbol, value) in &rates.0 {
                    NativeTokenRateByCurrency::<T>::insert(symbol, value.clone());
                }

                ProcessedNonces::<T>::put(nonce);
                LastPriceSubmission::<T>::put(<frame_system::Pallet<T>>::block_number());
                Nonce::<T>::mutate(|value| *value += 1);
            }
            Ok(().into())
        }

        #[pallet::call_index(1)]
        #[pallet::weight(Weight::default())]
        pub fn clear_consensus(
            origin: OriginFor<T>,
            submitter: Validator<T::AuthorityId, T::AccountId>,
            _signature: <<T as avn::Config>::AuthorityId as RuntimeAppPublic>::Signature,
        ) -> DispatchResultWithPostInfo {
            ensure_none(origin)?;

            ensure!(
                AVN::<T>::is_validator(&submitter.account_id),
                Error::<T>::SubmitterNotAValidator
            );

            let current_block = <frame_system::Pallet<T>>::block_number();
            let last_submission_block = LastPriceSubmission::<T>::get();

            let required_block = last_submission_block
                .saturating_add(BlockNumberFor::<T>::from(T::PriceRefreshRangeInBlocks::get()))
                .saturating_add(BlockNumberFor::<T>::from(T::ConsensusGracePeriod::get()));

            ensure!(current_block >= required_block, Error::<T>::GracePeriodNotPassed);

            let new_last_submission_block = current_block
                .saturating_sub(BlockNumberFor::<T>::from(T::PriceRefreshRangeInBlocks::get()));
            LastPriceSubmission::<T>::put(new_last_submission_block);

            let cleared_period = Nonce::<T>::get();
            Nonce::<T>::mutate(|value| *value += 1);

            Self::deposit_event(Event::<T>::ConsensusCleared { period: cleared_period });
            Ok(().into())
        }

        #[pallet::call_index(2)]
        #[pallet::weight(Weight::default())]
        pub fn register_currency(
            origin: OriginFor<T>,
            symbol: Vec<u8>,
        ) -> DispatchResultWithPostInfo {
            ensure_root(origin)?;

            let current_count = CurrencySymbols::<T>::iter().count() as u32;
            ensure!(current_count < T::MaxCurrencies::get(), Error::<T>::TooManyCurrencies);

            let currency_symbol = CurrencySymbol(symbol.clone());
            CurrencySymbols::<T>::insert(currency_symbol.clone(), ());

            Self::deposit_event(Event::<T>::CurrencyRegistered { symbol });
            Ok(().into())
        }

        #[pallet::call_index(3)]
        #[pallet::weight(Weight::default())]
        pub fn remove_currency(
            origin: OriginFor<T>,
            symbol: Vec<u8>,
        ) -> DispatchResultWithPostInfo {
            ensure_root(origin)?;

            let currency_symbol = CurrencySymbol(symbol.clone());
            ensure!(
                CurrencySymbols::<T>::contains_key(&currency_symbol),
                Error::<T>::CurrencyNotFound
            );
            CurrencySymbols::<T>::remove(&currency_symbol);

            Self::deposit_event(Event::<T>::CurrencyRemoved { symbol });
            Ok(().into())
        }
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_initialize(n: BlockNumberFor<T>) -> Weight {
            let total_weight = Weight::zero();

            let last_submission_block = LastPriceSubmission::<T>::get();
            let nonce = Nonce::<T>::get();
            if (n >=
                last_submission_block +
                    BlockNumberFor::<T>::from(T::PriceRefreshRangeInBlocks::get())) &&
                !PriceSubmissionTimestamps::<T>::contains_key(nonce)
            {
                let now = pallet_timestamp::Pallet::<T>::now();
                let now_u64: u64 = now.try_into().unwrap_or_default();
                let now_secs = now_u64 / 1000;
                let two_minutes_secs = 120;

                // we do this to ensure all data for the given period is available and the data is
                // consistent
                let to_u64 = now_secs.saturating_sub(two_minutes_secs);

                // 10 minutes
                let ninety_minutes_secs = 600;
                let from_u64 = to_u64.saturating_sub(ninety_minutes_secs);
                PriceSubmissionTimestamps::<T>::insert(nonce, (from_u64, to_u64));

                // TODO
                // total_weight = total_weight.saturating_add(
                //     <T as Config>::WeightInfo::on_initialize_updates_fiat_rates_query_timestamps(),
                // );
            }

            // let db_read_weight = T::DbWeight::get().reads(1);
            // total_weight.saturating_add(db_read_weight.saturating_mul(if_counter.into()))
            total_weight
        }

        fn offchain_worker(block_number: BlockNumberFor<T>) {
            log::info!(
                "Vow prices manager OCW -> 🚧 🚧 Running offchain worker for block: {:?}",
                block_number
            );

            let setup_result = AVN::<T>::pre_run_setup(block_number, PALLET_NAME.to_vec());
            if let Err(e) = setup_result {
                match e {
                    _ if e == DispatchError::from(avn_error::<T>::OffchainWorkerAlreadyRun) => {
                        ();
                    },
                    _ => {
                        log::error!("💔 Unable to run offchain worker: {:?}", e);
                    },
                };
                return
            }
            let (this_validator, _) = setup_result.expect("We have a validator");

            let _ = Self::submit_price_if_required(&this_validator);
            let _ = Self::clear_consensus_if_required(&this_validator, block_number);
        }

        fn on_idle(_now: BlockNumberFor<T>, limit: Weight) -> Weight {
            let mut meter = WeightMeter::with_limit(limit / 2);
            let min_on_idle_weight = Weight::zero(); // todo calculate weight

            if !meter.can_consume(min_on_idle_weight) {
                log::debug!("⚠️ Not enough weight to proceed with cleanup.");
                return meter.consumed();
            }

            let (mut price_reporters_nonce, mut prices_nonce) =
                LastClearedNonces::<T>::get().unwrap_or((0, 0));

            let max_vow_price_nonce_to_delete = ProcessedNonces::<T>::get();

            // Exit early if we've already caught up
            if price_reporters_nonce >= max_vow_price_nonce_to_delete &&
                prices_nonce >= max_vow_price_nonce_to_delete
            {
                return meter.consumed();
            }

            for _n in 0..MAX_DELETE_ATTEMPTS {
                if !meter.can_consume(min_on_idle_weight) {
                    break;
                }

                if price_reporters_nonce < max_vow_price_nonce_to_delete {
                    let cleared = PriceReporters::<T>::drain_prefix(price_reporters_nonce)
                        .take(BATCH_PER_STORAGE)
                        .count();
                    if cleared < BATCH_PER_STORAGE {
                        price_reporters_nonce += 1;
                    }
                }

                if prices_nonce < max_vow_price_nonce_to_delete {
                    let cleared = ReportedRates::<T>::drain_prefix(prices_nonce)
                        .take(BATCH_PER_STORAGE)
                        .count();
                    if cleared < BATCH_PER_STORAGE {
                        prices_nonce += 1;
                    }
                }

                meter.consume(min_on_idle_weight);
            }

            LastClearedNonces::<T>::put((price_reporters_nonce, prices_nonce));
            meter.consumed()
        }
    }

    #[pallet::validate_unsigned]
    impl<T: Config> ValidateUnsigned for Pallet<T> {
        type Call = Call<T>;
        fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
            match call {
                Call::submit_price { rates, submitter, signature } =>
                    if AVN::<T>::signature_is_valid(
                        &(PRICE_SUBMISSION_CONTEXT, rates, Nonce::<T>::get()),
                        &submitter,
                        signature,
                    ) {
                        ValidTransaction::with_tag_prefix("SubmitAvtPrice")
                            .and_provides(vec![(
                                PRICE_SUBMISSION_CONTEXT,
                                rates,
                                Nonce::<T>::get(),
                                submitter.account_id.clone(),
                            )
                                .encode()])
                            .priority(TransactionPriority::max_value())
                            .longevity(64_u64)
                            .propagate(true)
                            .build()
                    } else {
                        InvalidTransaction::Custom(1u8).into()
                    },
                Call::clear_consensus { submitter, signature } =>
                    if AVN::<T>::signature_is_valid(
                        &(CLEAR_CONSENSUS_SUBMISSION_CONTEXT, Nonce::<T>::get()),
                        &submitter,
                        signature,
                    ) {
                        ValidTransaction::with_tag_prefix("ClearConsensus")
                            .and_provides(vec![(
                                CLEAR_CONSENSUS_SUBMISSION_CONTEXT,
                                Nonce::<T>::get(),
                                submitter.account_id.clone(),
                            )
                                .encode()])
                            .priority(TransactionPriority::max_value())
                            .longevity(64_u64)
                            .propagate(true)
                            .build()
                    } else {
                        InvalidTransaction::Custom(1u8).into()
                    },
                _ => InvalidTransaction::Call.into(),
            }
        }
    }

    impl<T: Config> Pallet<T> {
        fn submit_price_if_required(
            submitter: &Validator<T::AuthorityId, T::AccountId>,
        ) -> Result<(), DispatchError> {
            if !Self::should_query_rates() {
                return Ok(());
            }

            let current_block = <frame_system::Pallet<T>>::block_number();
            let last_submission_block = LastPriceSubmission::<T>::get();
            let nonce = Nonce::<T>::get();

            let guard_lock_name =
                Self::create_guard_lock(b"submit_price::", nonce, &submitter.account_id);

            if current_block >=
                last_submission_block +
                    BlockNumberFor::<T>::from(T::PriceRefreshRangeInBlocks::get())
            {
                let mut lock = AVN::<T>::get_ocw_locker(&guard_lock_name);
                if let Ok(guard) = lock.try_lock() {
                    let rates = Self::fetch_and_decode_rates()?;
                    let signature = submitter
                        .key
                        .sign(&(PRICE_SUBMISSION_CONTEXT, rates.clone(), nonce).encode())
                        .ok_or(Error::<T>::ErrorSigning);

                    let _ = SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(
                        Call::submit_price {
                            rates,
                            submitter: submitter.clone(),
                            signature: signature.expect("checked for errors"),
                        }
                        .into(),
                    )
                    .map_err(|_| Error::<T>::ErrorSubmittingTransaction);

                    guard.forget();
                };
            }

            Ok(())
        }

        fn clear_consensus_if_required(
            submitter: &Validator<T::AuthorityId, T::AccountId>,
            current_block: BlockNumberFor<T>,
        ) -> Result<(), DispatchError> {
            if !Self::should_query_rates() {
                return Ok(());
            }

            let last_submission_block = LastPriceSubmission::<T>::get();

            if current_block >=
                last_submission_block
                    .saturating_add(BlockNumberFor::<T>::from(T::PriceRefreshRangeInBlocks::get()))
                    .saturating_add(BlockNumberFor::<T>::from(T::ConsensusGracePeriod::get()))
            {
                let signature = submitter
                    .key
                    .sign(&(CLEAR_CONSENSUS_SUBMISSION_CONTEXT, Nonce::<T>::get()).encode())
                    .ok_or(Error::<T>::ErrorSigning);

                let _ = SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(
                    Call::clear_consensus {
                        submitter: submitter.clone(),
                        signature: signature.expect("checked for errors"),
                    }
                    .into(),
                )
                .map_err(|_| Error::<T>::ErrorSubmittingTransaction);
            }
            Ok(())
        }

        pub fn create_guard_lock<BlockNumber: Encode>(
            prefix: &'static [u8],
            block_number: BlockNumber,
            authority: &T::AccountId,
        ) -> Vec<u8> {
            let mut name = prefix.to_vec();
            name.extend_from_slice(&block_number.encode());
            name.extend_from_slice(&authority.encode());
            name
        }

        fn fetch_and_decode_rates() -> Result<Rates, DispatchError> {
            let stored_currencies: Vec<String> = CurrencySymbols::<T>::iter_keys()
                .map(|s| String::from_utf8_lossy(&s.0).into_owned())
                .collect();

            let nonce = Nonce::<T>::get();
            let (from, to) = PriceSubmissionTimestamps::<T>::get(nonce)
                .ok_or(Error::<T>::MissingPriceTimestamps)?;

            let endpoint = format!(
                "/get_token_rates/aventus/{}/{}/{}",
                stored_currencies.join(","),
                from,
                to,
            );
            let response = AVN::<T>::get_data_from_service(endpoint)
                .map_err(|_| Error::<T>::ErrorFetchingPrice)?;

            let formatted = Self::format_rates(response);
            log::info!("✅ Formatted Rates: {:?}", formatted);

            formatted
        }

        fn format_rates(prices_json: Vec<u8>) -> Result<Rates, DispatchError> {
            let prices_str = String::from_utf8_lossy(&prices_json);
            let prices: Value = serde_json::from_str(&prices_str)
                .map_err(|_| DispatchError::Other("JSON Parsing Error"))?;

            let mut formatted_rates: Vec<(CurrencySymbol, U256)> = Vec::new();

            if let Some(rates) = prices.as_object() {
                if let Some((symbol, rate_value)) = rates.iter().next() {
                    if let Some(rate) = rate_value.as_f64() {
                        if rate <= 0.0 {
                            return Err(DispatchError::Other(
                                Error::<T>::PriceMustBeGreaterThanZero.into(),
                            ))
                        }
                        let scaled_rate = U256::from((rate * 1e8) as u128);
                        let symbol_key = CurrencySymbol(symbol.as_bytes().to_vec());
                        formatted_rates.push((symbol_key, scaled_rate));
                    } else {
                        return Err(Error::<T>::InvalidRateFormat.into())
                    }
                }
            }
            Ok(Rates(formatted_rates))
        }

        pub fn should_query_rates() -> bool {
            CurrencySymbols::<T>::iter_keys().next().is_some()
        }
    }
}

#[cfg(any(test, feature = "runtime-benchmarks"))]
mod mock;
#[cfg(test)]
mod tests;
