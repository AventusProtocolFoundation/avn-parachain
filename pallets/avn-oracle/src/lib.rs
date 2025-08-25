pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    use frame_support::pallet_prelude::*;
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
    use sp_runtime::{DispatchError, RuntimeAppPublic};

    const PALLET_NAME: &'static [u8] = b"AvnOracle";
    pub const AVT_PRICE_SUBMISSION_CONTEXT: &'static [u8] = b"update_avt_price_signing_context";

    pub type AVN<T> = avn::Pallet<T>;

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    #[pallet::getter(fn last_avt_price_submission)]
    pub type LastAvtPriceSubmission<T: Config> = StorageValue<_, BlockNumberFor<T>, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn avt_price_nonce)]
    pub type AvtPriceNonce<T> = StorageValue<_, u32, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn avt_price_submission_timestamps)]
    pub type AvtPriceSubmissionTimestamps<T: Config> =
        StorageMap<_, Blake2_128Concat, u32, (u64, u64), OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn avt_price_reporters)]
    pub type AvtPriceReporters<T: Config> =
        StorageDoubleMap<_, Blake2_128Concat, u32, Blake2_128Concat, T::AccountId, (), ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn processed_avt_price_nonces)]
    pub type ProcessedAvtPriceNonces<T: Config> = StorageValue<_, u32, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn reported_avt_price)]
    pub type ReportedAvtPrice<T: Config> =
        StorageDoubleMap<_, Blake2_128Concat, u32, Blake2_128Concat, U256, u32, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn aventus_price_in_usd)]
    pub type AventusUsdPrice<T> = StorageValue<_, U256, OptionQuery>;

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
        type AvtPriceRefreshRangeInBlocks: Get<u32>;
    }

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        AventusPriceUpdated { avt_price: U256 },
    }

    #[pallet::error]
    pub enum Error<T> {
        SubmitterNotAValidator,
        ErrorSigning,
        ErrorSubmittingTransaction,
        ErrorFetchingAvtPrice,
        ValidatorAlreadySubmitted,
        AvtPriceMustBeGreaterThanZero,
        InvalidRateFormat,
        MissingAvtPriceTimestamps,
    }

    #[pallet::storage]
    pub type Something<T> = StorageValue<_, u32>;

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(Weight::default())]
        pub fn submit_avt_price(
            origin: OriginFor<T>,
            avt_price: U256,
            submitter: Validator<T::AuthorityId, T::AccountId>,
            _signature: <<T as avn::Config>::AuthorityId as RuntimeAppPublic>::Signature,
        ) -> DispatchResultWithPostInfo {
            ensure_none(origin)?;
            ensure!(
                AVN::<T>::is_validator(&submitter.account_id),
                Error::<T>::SubmitterNotAValidator
            );

            let nonce = AvtPriceNonce::<T>::get();
            ensure!(
                !AvtPriceReporters::<T>::contains_key(nonce, &submitter.account_id),
                Error::<T>::ValidatorAlreadySubmitted
            );
            AvtPriceReporters::<T>::insert(nonce, &submitter.account_id, ());

            let count = ReportedAvtPrice::<T>::mutate(nonce, &avt_price, |count| {
                *count = count.saturating_add(1);
                *count
            });

            if count > AVN::<T>::quorum() {
                log::info!("🎁 Quorum reached: {}, proceeding to publish rates", count);
                Self::deposit_event(Event::<T>::AventusPriceUpdated { avt_price });

                AventusUsdPrice::<T>::put(avt_price);

                ProcessedAvtPriceNonces::<T>::put(nonce);
                LastAvtPriceSubmission::<T>::put(<frame_system::Pallet<T>>::block_number());
                AvtPriceNonce::<T>::mutate(|value| *value += 1);
            }

            Ok(().into())
        }
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_initialize(n: BlockNumberFor<T>) -> Weight {
            let total_weight = Weight::zero();

            let last_submission_block = LastAvtPriceSubmission::<T>::get();
            let nonce = AvtPriceNonce::<T>::get();
            if (n >=
                last_submission_block +
                    BlockNumberFor::<T>::from(T::AvtPriceRefreshRangeInBlocks::get())) &&
                !AvtPriceSubmissionTimestamps::<T>::contains_key(nonce)
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
                AvtPriceSubmissionTimestamps::<T>::insert(nonce, (from_u64, to_u64));

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

            let _ = Self::submit_avt_price_if_required(&this_validator);
        }
    }

    #[pallet::validate_unsigned]
    impl<T: Config> ValidateUnsigned for Pallet<T> {
        type Call = Call<T>;
        fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
            match call {
                Call::submit_avt_price { avt_price, submitter, signature } =>
                    if AVN::<T>::signature_is_valid(
                        &(AVT_PRICE_SUBMISSION_CONTEXT, avt_price, AvtPriceNonce::<T>::get()),
                        &submitter,
                        signature,
                    ) {
                        ValidTransaction::with_tag_prefix("SubmitAvtPrice")
                            .and_provides(vec![(
                                AVT_PRICE_SUBMISSION_CONTEXT,
                                avt_price,
                                AvtPriceNonce::<T>::get(),
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
        fn submit_avt_price_if_required(
            submitter: &Validator<T::AuthorityId, T::AccountId>,
        ) -> Result<(), DispatchError> {
            let current_block = <frame_system::Pallet<T>>::block_number();
            let last_submission_block = LastAvtPriceSubmission::<T>::get();

            if current_block >=
                last_submission_block +
                    BlockNumberFor::<T>::from(T::AvtPriceRefreshRangeInBlocks::get())
            {
                let guard_lock_name = Self::create_guard_lock(
                    b"submit_avt_price::",
                    AvtPriceNonce::<T>::get(),
                    &submitter.account_id,
                );
                let mut lock = AVN::<T>::get_ocw_locker(&guard_lock_name);

                if let Ok(guard) = lock.try_lock() {
                    let avt_price = Self::fetch_and_decode_avt_price()?;
                    let signature = submitter
                        .key
                        .sign(
                            &(
                                AVT_PRICE_SUBMISSION_CONTEXT,
                                avt_price.clone(),
                                AvtPriceNonce::<T>::get(),
                            )
                                .encode(),
                        )
                        .ok_or(Error::<T>::ErrorSigning);

                    let _ = SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(
                        Call::submit_avt_price {
                            avt_price,
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

        fn fetch_and_decode_avt_price() -> Result<U256, DispatchError> {
            let nonce = AvtPriceNonce::<T>::get();
            let (from, to) = AvtPriceSubmissionTimestamps::<T>::get(nonce)
                .ok_or(Error::<T>::MissingAvtPriceTimestamps)?;

            let endpoint = format!("/get_fiat_rates/aventus/usd/{}/{}", from, to,);
            let response = AVN::<T>::get_data_from_service(endpoint)
                .map_err(|_| Error::<T>::ErrorFetchingAvtPrice)?;

            let formatted = Self::format_avt_price(response);
            log::info!("✅ Formatted FiatRates: {:?}", formatted);

            formatted
        }

        fn format_avt_price(fiat_rates_json: Vec<u8>) -> Result<U256, DispatchError> {
            let fiat_rates_str = String::from_utf8_lossy(&fiat_rates_json);
            let fiat_rates: Value = serde_json::from_str(&fiat_rates_str)
                .map_err(|_| DispatchError::Other("JSON Parsing Error"))?;

            if let Some(rates) = fiat_rates.as_object() {
                if let Some((_symbol, rate_value)) = rates.iter().next() {
                    if let Some(rate) = rate_value.as_f64() {
                        if rate <= 0.0 {
                            return Err(DispatchError::Other(
                                Error::<T>::AvtPriceMustBeGreaterThanZero.into(),
                            ));
                        }
                        // just scale and return
                        let scaled_rate = U256::from((rate * 1e8) as u128);
                        return Ok(scaled_rate);
                    } else {
                        return Err(Error::<T>::InvalidRateFormat.into());
                    }
                }
            }
            return Err(Error::<T>::InvalidRateFormat.into());
        }
    }
}
