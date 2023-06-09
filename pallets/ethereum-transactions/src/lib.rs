// Copyright 2022 Aventus Network Services (UK) Ltd.

#![cfg_attr(not(feature = "std"), no_std)]
#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{dispatch::DispatchResult, ensure, log, traits::Get};
use frame_system::{
    self as system, ensure_none, ensure_root,
    offchain::{SendTransactionTypes, SubmitTransaction},
};
use sp_application_crypto::RuntimeAppPublic;
use sp_runtime::{
    offchain::{http, Duration},
    scale_info::TypeInfo,
    traits::{AtLeast32Bit, Member},
    transaction_validity::{
        InvalidTransaction, TransactionPriority, TransactionSource, TransactionValidity,
        ValidTransaction,
    },
    DispatchError,
};
use sp_std::prelude::*;

use sp_avn_common::{
    event_types::Validator,
    offchain_worker_storage_lock::{self as OcwLock, OcwOperationExpiration},
    EthTransaction,
};
use sp_core::{ecdsa, H160, H256};

use core::convert::TryInto;
pub use pallet::*;

pub mod ethereum_transaction;
use crate::ethereum_transaction::{
    EthTransactionCandidate, EthTransactionType, EthereumTransactionHash, TransactionId,
};

use pallet_avn::{self as avn, AccountToBytesConverter, Error as avn_error};

pub type AVN<T> = avn::Pallet<T>;

#[cfg(test)]
mod mock;

#[cfg(test)]
#[path = "tests/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/tests_validate_unsigned.rs"]
mod validate_unsigned;

#[cfg(test)]
#[path = "tests/ethereum_transaction_tests.rs"]
mod ethereum_transaction_tests;

#[cfg(test)]
#[path = "tests/tests_eth_transaction_type.rs"]
mod tests_eth_transaction_type;

#[cfg(test)]
#[path = "tests/test_set_publish_root_contract.rs"]
mod test_set_publish_root_contract;

mod benchmarking;

pub mod default_weights;
pub use default_weights::WeightInfo;

const NAME: &'static [u8; 26] = b"eth_transactions::last_run";
const SET_ETH_TX_HASH_FOR_DISPATCHED_TX: &'static [u8] = b"set_eth_tx_hash_for_dispatched_tx";

const SUBMITTER_IS_NOT_VALIDATOR: u8 = 1;
// Avoid sending multiple concurrent requests to avn-service at once. Set a throttle to 1.
const MAX_VALUES_RETURNED: usize = 1;
const MAX_VALIDATORS: u32 = 10; // used in benchmarks and weights calculation only
const MAX_TXS_PER_ACCOUNT: u32 = 1_000_000; // used in benchmarks and weights calculation only

// TODO [TYPE: business logic][PRI: high][CRITICAL][JIRA: 354] investigate the time needed for an
// ethereum transaction to become stale.

// TODO [JIRA: SYS-598] Will make this a configurable item for a node, so the validator can choose
// the frequency of resend. As a default value we set 10 minutes: 10 * 60 = 600 seconds
// AvN Parachain blocks are every 12 seconds therefore 600 / 12 = 50
const ETHEREUM_SEND_BLOCKS_EXPIRY: u32 = 50;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::{pallet_prelude::*, Blake2_128Concat};
    use frame_system::pallet_prelude::*;

    // Public interface of this pallet
    #[pallet::config]
    pub trait Config:
        SendTransactionTypes<Call<Self>> + frame_system::Config + avn::Config
    {
        type RuntimeEvent: From<Event<Self>>
            + Into<<Self as system::Config>::RuntimeEvent>
            + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        type RuntimeCall: From<Call<Self>>;

        type AccountToBytesConvert: AccountToBytesConverter<Self::AccountId>;

        type ValidatorManagerContractAddress: Get<H160>;

        type WeightInfo: WeightInfo;
    }

    #[pallet::pallet]
    #[pallet::generate_store(pub (super) trait Store)]
    #[pallet::without_storage_info]
    pub struct Pallet<T>(_);

    #[pallet::event]
    /// This attribute generate the function `deposit_event` to deposit one of this pallet event,
    /// it is optional, it is also possible to provide a custom implementation.
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        TransactionReadyToSend {
            transaction_id: TransactionId,
            sender: T::AccountId,
        },
        EthereumTransactionHashAdded {
            transaction_id: TransactionId,
            transaction_hash: EthereumTransactionHash,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        TransactionExists,
        NotEnoughConfirmations,
        ErrorSigning,
        ErrorSubmittingTransaction,
        InvalidKey,
        EthTransactionHashValueMutableOnce,
        MissingDispatchedAvnTx,
        MissingDispatchedAvnTxSubmitter,
        InvalidTransactionSubmitter,
        InvalidHexString,
        InvalidHashLength,
        InvalidConfirmations,
        ReservedMissing,
        ReservedMismatch,
        // SYS-396 TODO Drop the HTTP errors
        //TODO [TYPE: refactoring][PRI: low]: These could be stored in a central place and used
        // for all http requests
        DeadlineReached,
        InvalidUTF8Bytes,
        RequestTimedOut,
        UnexpectedStatusCode,
        InvalidContractAddress,
    }

    #[pallet::storage]
    // TODO [TYPE: business logic][PRI: medium][CRITICAL][JIRA: 352] refactor this area:
    // - We need another storage to record confirmation from the external service that this
    //   transaction has been submitted
    #[pallet::getter(fn get_transaction)]
    pub type Repository<T: Config> =
        StorageMap<_, Blake2_128Concat, TransactionId, EthTransactionCandidate, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn get_dispatched_avn_tx_ids)]
    pub type DispatchedAvnTxIds<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Vec<DispatchedData<T::BlockNumber>>,
        ValueQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn get_reserved)]
    pub type ReservedTransactions<T: Config> =
        StorageMap<_, Blake2_128Concat, EthTransactionType, TransactionId, ValueQuery>;

    // TODO [TYPE: refactoring][PRI: low] use a map to store all contract address
    // pub ContractAddresses get(fn get_contract_address): map hasher(blake2_128_concat)
    // TransactionId => H160;
    #[pallet::storage]
    #[pallet::getter(fn get_publish_root_contract)]
    pub type PublishRootContract<T: Config> = StorageValue<_, H160, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn get_nonce)]
    pub type Nonce<T: Config> = StorageValue<_, TransactionId, ValueQuery>;

    #[pallet::genesis_config]
    pub struct GenesisConfig<T: Config> {
        pub _phantom: sp_std::marker::PhantomData<T>,
        pub get_publish_root_contract: H160,
    }

    #[cfg(feature = "std")]
    impl<T: Config> Default for GenesisConfig<T> {
        fn default() -> Self {
            Self { _phantom: Default::default(), get_publish_root_contract: H160::zero() }
        }
    }

    #[pallet::genesis_build]
    impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
        fn build(&self) {
            PublishRootContract::<T>::put(self.get_publish_root_contract);
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        // TODO [TYPE: business logic][PRI: medium]: This is a workaround to allow synch with T1
        // when we reset T2. This is a Sudo call and as such should not be in the production
        // code. Check if we can remove it already. This is needed while we are not
        // finalized, and possible in a state where our governance is centralized.
        // Suggestion: We can wrap it in a build configuration flag for conditional compilation, eg
        // "allow-sudo-shortcuts"
        #[pallet::call_index(0)]
        #[pallet::weight(<T as pallet::Config>::WeightInfo::set_transaction_id())]
        pub fn set_transaction_id(
            origin: OriginFor<T>,
            transaction_id: TransactionId,
        ) -> DispatchResult {
            ensure_root(origin)?;
            <Nonce<T>>::put(transaction_id);
            Ok(())
        }

        #[pallet::call_index(1)]
        #[pallet::weight(<T as pallet::Config>::WeightInfo::set_eth_tx_hash_for_dispatched_tx(MAX_VALIDATORS, MAX_TXS_PER_ACCOUNT))]
        pub fn set_eth_tx_hash_for_dispatched_tx(
            origin: OriginFor<T>,
            submitter: T::AccountId,
            candidate_tx_id: TransactionId,
            eth_tx_hash: EthereumTransactionHash,
            _signature: <T::AuthorityId as RuntimeAppPublic>::Signature,
        ) -> DispatchResult {
            ensure_none(origin)?;
            ensure!(AVN::<T>::is_validator(&submitter), Error::<T>::InvalidKey);
            ensure!(
                Self::get_transaction(candidate_tx_id).from ==
                    Some(T::AccountToBytesConvert::into_bytes(&submitter)),
                Error::<T>::InvalidTransactionSubmitter
            );
            ensure!(
                DispatchedAvnTxIds::<T>::contains_key(&submitter),
                Error::<T>::MissingDispatchedAvnTxSubmitter
            );
            ensure!(
                Self::get_dispatched_avn_tx_ids(submitter)
                    .iter()
                    .any(|data| data.transaction_id == candidate_tx_id),
                Error::<T>::MissingDispatchedAvnTx
            );

            let _ = <Repository<T>>::mutate(candidate_tx_id, |storage_candidate| {
                storage_candidate.set_eth_tx_hash::<T>(eth_tx_hash)
            })?;

            Self::deposit_event(Event::<T>::EthereumTransactionHashAdded {
                transaction_id: candidate_tx_id,
                transaction_hash: eth_tx_hash,
            });

            // TODO [TYPE: weightInfo][PRI: medium]: Return accurate weight
            Ok(())
        }

        // See SYS-870 & SYS-855 for more information
        /// Removes a reservation for a transaction that was created with reserve_transaction_id
        /// Only sudo should call this to repair a network.
        #[pallet::call_index(2)]
        #[pallet::weight(<T as pallet::Config>::WeightInfo::unreserve_transaction())]
        pub fn unreserve_transaction(
            origin: OriginFor<T>,
            transaction_type: EthTransactionType,
        ) -> DispatchResult {
            ensure_root(origin)?;
            if <ReservedTransactions<T>>::contains_key(&transaction_type) {
                let reserved_tx_id = Self::get_reserved(&transaction_type);
                <ReservedTransactions<T>>::remove(&transaction_type);
                <ReservedTransactions<T>>::insert(
                    EthTransactionType::Discarded(reserved_tx_id),
                    reserved_tx_id,
                );
            }
            Ok(())
        }

        /// Sets the address for ethereum contracts
        #[pallet::call_index(3)]
        #[pallet::weight(<T as pallet::Config>::WeightInfo::set_publish_root_contract())]
        pub fn set_publish_root_contract(
            origin: OriginFor<T>,
            contract_address: H160,
        ) -> DispatchResult {
            ensure_root(origin)?;
            ensure!(&contract_address != &H160::zero(), Error::<T>::InvalidContractAddress);

            <PublishRootContract<T>>::put(contract_address);
            Ok(())
        }
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        /// Offchain Worker entry point.
        fn offchain_worker(block_number: T::BlockNumber) {
            let setup_result = AVN::<T>::pre_run_setup(block_number, NAME.to_vec());
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
            let this_validator = setup_result.expect("We have a validator");

            // ====================== Choose Offchain-Worker Action ===============
            Self::send_transaction_candidates(&this_validator, block_number);

            // TODO [TYPE: review][PRI: high][CRITICAL][JIRA: 352] add the rest offchain worker
            // logic here, corresponding to the confirmation loop (eg transactions sent
            // to Ethereum)
        }
    }

    #[pallet::validate_unsigned]
    impl<T: Config> ValidateUnsigned for Pallet<T> {
        // https://substrate.dev/rustdocs/v2.0.0-rc3/sp_runtime/traits/trait.ValidateUnsigned.html
        type Call = Call<T>;

        // TODO [TYPE: security][PRI: high][CRITICAL][JIRA: 152]: Make sure we are not open to
        // transaction replay attacks, or signature re-use.
        fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
            if let Call::set_eth_tx_hash_for_dispatched_tx {
                submitter,
                candidate_tx_id,
                eth_tx_hash,
                signature,
            } = call
            {
                let data_to_sign = (&submitter, &candidate_tx_id, eth_tx_hash);
                let submitter_validator = AVN::<T>::try_get_validator(&submitter);

                if submitter_validator.is_none() {
                    return InvalidTransaction::Custom(SUBMITTER_IS_NOT_VALIDATOR).into()
                }
                if !AVN::<T>::signature_is_valid(
                    &(SET_ETH_TX_HASH_FOR_DISPATCHED_TX, data_to_sign),
                    &submitter_validator.expect("If it got here, its not none"),
                    signature,
                ) {
                    return InvalidTransaction::BadProof.into()
                };

                ValidTransaction::with_tag_prefix("EthereumTransactions")
                    .priority(TransactionPriority::max_value())
                    .and_provides(vec![(
                        SET_ETH_TX_HASH_FOR_DISPATCHED_TX,
                        submitter,
                        candidate_tx_id,
                        eth_tx_hash,
                    )
                        .encode()])
                    .longevity(64_u64)
                    .propagate(true)
                    .build()
            } else {
                return InvalidTransaction::Call.into()
            }
        }
    }
}

impl<T: Config> Pallet<T> {
    fn get_unique_transaction_identifier() -> TransactionId {
        let id = <Nonce<T>>::get();
        <Nonce<T>>::mutate(|n| *n += 1);
        id
    }

    //TODO [TYPE: refactoring][PRI: medium]: These methods can be extracted into a separate module
    fn transactions_ready_to_be_sent(
        account_id: &T::AccountId,
    ) -> Vec<(TransactionId, EthTransaction)> {
        Self::get_dispatched_avn_tx_ids(account_id)
            .into_iter()
            .filter_map(|data| Self::get_transaction_to_send_if_available(data, account_id))
            .take(MAX_VALUES_RETURNED)
            .collect()
    }

    fn get_transaction_to_send_if_available(
        dispatched_data: DispatchedData<T::BlockNumber>,
        account_id: &T::AccountId,
    ) -> Option<(TransactionId, EthTransaction)> {
        if !<Repository<T>>::contains_key(dispatched_data.transaction_id) ||
            Self::is_transaction_locked_for_sending(&dispatched_data.transaction_id)
        {
            return None
        }

        let transaction = Self::get_transaction(dispatched_data.transaction_id);

        if transaction.from == Some(T::AccountToBytesConvert::into_bytes(account_id)) &&
            transaction.get_eth_tx_hash().is_none()
        {
            let ethereum_contract = Self::get_contract_address(&transaction.call_data);
            if ethereum_contract.is_none() {
                log::error!("Invalid transaction type");
                return None
            }

            let eth_transaction = transaction.to_abi(ethereum_contract.expect("Checked for error"));
            if let Err(e) = eth_transaction {
                log::error!("Error abi encoding: {:#?}, error: {:#?}", transaction, e);
                return None
            }

            // It is only safe to proceed if the block number the dispatch request was added is
            // finalised. Otherwise we might be vulnerable to re-orgs
            if !AVN::<T>::is_block_finalised(dispatched_data.submitted_at_block) {
                log::error!(
                    "Block number {:?} is not finalised yet",
                    dispatched_data.submitted_at_block
                );
                return None
            }

            return Some((transaction.tx_id, eth_transaction.expect("Checked for error")))
        }

        return None
    }

    fn get_contract_address(transaction_type: &EthTransactionType) -> Option<H160> {
        return match transaction_type {
            EthTransactionType::PublishRoot(_) => Some(Self::get_publish_root_contract()),
            EthTransactionType::ActivateCollator(_) | EthTransactionType::DeregisterCollator(_) =>
                Some(T::ValidatorManagerContractAddress::get()),
            _ => None,
        }
    }

    fn promote_candidate_transaction_to_dispatched(
        submitter: T::AccountId,
        candidate_tx_id: TransactionId,
    ) {
        let candidate_tx = Self::get_transaction(candidate_tx_id);
        if candidate_tx.ready_to_dispatch() {
            if <DispatchedAvnTxIds<T>>::contains_key(&submitter) {
                <DispatchedAvnTxIds<T>>::mutate(&submitter, |submitter_dispatched_tx| {
                    submitter_dispatched_tx.push(DispatchedData::new(
                        candidate_tx_id,
                        <system::Pallet<T>>::block_number(),
                    ))
                });
            } else {
                <DispatchedAvnTxIds<T>>::insert(
                    &submitter,
                    vec![DispatchedData::new(candidate_tx_id, <system::Pallet<T>>::block_number())],
                );
            }
            Self::deposit_event(Event::<T>::TransactionReadyToSend {
                transaction_id: candidate_tx.tx_id,
                sender: submitter,
            });
        }
    }

    // TODO [TYPE: refactoring][PRI: medium]: Centralise logic, possibly into a separate service
    // helper module
    pub fn send_transaction_to_ethereum(
        transaction_to_send: EthTransaction,
    ) -> Result<H256, DispatchError> {
        let deadline = sp_io::offchain::timestamp().add(Duration::from_millis(2_000));
        let external_service_port_number = AVN::<T>::get_external_service_port_number();

        let mut url = String::from("http://127.0.0.1:");
        url.push_str(&external_service_port_number);
        url.push_str(&"/eth/send".to_string());

        let pending = http::Request::default()
            .deadline(deadline)
            .method(http::Method::Post)
            .url(&url)
            .body(vec![transaction_to_send.encode()])
            .send()
            .map_err(|_| Error::<T>::RequestTimedOut)?;

        let response = pending
            .try_wait(deadline)
            .map_err(|_| Error::<T>::DeadlineReached)?
            .map_err(|_| Error::<T>::DeadlineReached)?;

        if response.code != 200 {
            log::error!("❌ Unexpected status code: {}", response.code);
            return Err(Error::<T>::UnexpectedStatusCode)?
        }

        let result: Vec<u8> = response.body().collect::<Vec<u8>>();
        if result.len() != 64 {
            log::error!("❌ Ethereum transaction hash is not valid: {:?}", result);
            return Err(Error::<T>::InvalidHashLength)?
        }

        let tx_hash_string = core::str::from_utf8(&result);
        if let Err(e) = tx_hash_string {
            log::error!("❌ Error converting txHash bytes to string: {:?}", e);
            return Err(Error::<T>::InvalidUTF8Bytes)?
        }

        let mut data: [u8; 32] = [0; 32];
        hex::decode_to_slice(tx_hash_string.expect("Checked for error"), &mut data[..])
            .map_err(|_| Error::<T>::InvalidHexString)?;
        return Ok(H256::from_slice(&data))
    }

    // ============================ Helper functions that create unsigned transactions
    // ===================================

    fn issue_set_eth_tx_hash_for_dispatched_tx(
        candidate_tx_id: TransactionId,
        authority: &Validator<T::AuthorityId, T::AccountId>,
        eth_tx_hash: H256,
    ) -> Result<(), Error<T>> {
        let data_to_sign = (&authority.account_id, &candidate_tx_id, eth_tx_hash);

        let signature = authority
            .key
            .sign(&(SET_ETH_TX_HASH_FOR_DISPATCHED_TX, data_to_sign).encode())
            .ok_or(Error::<T>::ErrorSigning)?;

        SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(
            Call::set_eth_tx_hash_for_dispatched_tx {
                submitter: authority.account_id.clone(),
                candidate_tx_id,
                eth_tx_hash,
                signature,
            }
            .into(),
        )
        .map_err(|_| Error::<T>::ErrorSubmittingTransaction)?;

        Ok(())
    }

    // ================================= Offchain Worker Helpers
    // ========================================

    fn generate_sending_lock_name(candidate_id: TransactionId) -> OcwLock::PersistentId {
        let mut name = b"eth_transactions::lock::tx_id::".to_vec();
        name.extend_from_slice(&mut &candidate_id.to_le_bytes()[..]);
        name
    }

    fn is_transaction_locked_for_sending(candidate_id: &TransactionId) -> bool {
        let persistent_data = Self::generate_sending_lock_name(*candidate_id);
        return OcwLock::is_locked(&persistent_data)
    }

    fn send_transaction_candidates(
        authority: &Validator<T::AuthorityId, T::AccountId>,
        block_number: T::BlockNumber,
    ) {
        for (tx_id, eth_transaction) in Self::transactions_ready_to_be_sent(&authority.account_id) {
            if OcwLock::set_lock_with_expiry(
                block_number,
                OcwOperationExpiration::Custom(ETHEREUM_SEND_BLOCKS_EXPIRY),
                Self::generate_sending_lock_name(tx_id),
            )
            .is_err()
            {
                continue
            }

            // We don't send that often so an info log here should be ok.
            log::info!("ℹ️ Sending transaction (tx Id: {:?}) to Ethereum", tx_id);

            //TODO [TYPE: refactoring][PRI: low]: Code review comment - Think about creating a
            // wrapper function for these 2 methods if possible
            match Self::send_transaction_to_ethereum(eth_transaction) {
                Ok(eth_tx_hash) => {
                    let result = Self::issue_set_eth_tx_hash_for_dispatched_tx(
                        tx_id,
                        &authority,
                        eth_tx_hash,
                    );
                    if let Err(e) = result {
                        log::error!("Error updating avn transaction with eth tx hash: {:#?}", e);
                    }
                },
                Err(e) => {
                    log::info!("External service could not send transaction to Ethereum: {:?}", e);
                },
            }
        }
    }
}

pub trait CandidateTransactionSubmitter<AccountId> {
    /// Reserves a TransactionId for a transaction. When that transaction is submitted with
    /// submit_candidate_transaction_to_tier1, it must use the reserved TransactionId
    fn reserve_transaction_id(
        candidate_type: &EthTransactionType,
    ) -> Result<TransactionId, DispatchError>;

    /// Submits a candidate transaction with a reserved TransactionId and attached signatures.
    /// If the attached signatures don't match the needed quorum, the submission will get rejected.
    fn submit_candidate_transaction_to_tier1(
        candidate_type: EthTransactionType,
        tx_id: TransactionId,
        submitter: AccountId,
        signatures: Vec<ecdsa::Signature>,
    ) -> DispatchResult;

    /// Sets a transaction Id. This is only enabled for benchmarks
    #[cfg(feature = "runtime-benchmarks")]
    fn set_transaction_id(candidate_type: &EthTransactionType, id: TransactionId);

    // TODO review if we need an interface to change the value of EthTransactionType that has
    // reserved a TransactionId For example when a successful challenge occurs on the pallet
    // that reserved the tx_id.
}

impl<T: Config> CandidateTransactionSubmitter<T::AccountId> for Pallet<T> {
    fn reserve_transaction_id(
        candidate_type: &EthTransactionType,
    ) -> Result<TransactionId, DispatchError> {
        ensure!(
            !<ReservedTransactions<T>>::contains_key(candidate_type),
            Error::<T>::TransactionExists
        );

        let reserved_transaction_id = Self::get_unique_transaction_identifier();
        <ReservedTransactions<T>>::insert(candidate_type, reserved_transaction_id);

        return Ok(reserved_transaction_id)
    }

    fn submit_candidate_transaction_to_tier1(
        candidate_type: EthTransactionType,
        tx_id: TransactionId,
        submitter: T::AccountId,
        signatures: Vec<ecdsa::Signature>,
    ) -> DispatchResult {
        ensure!(
            <ReservedTransactions<T>>::contains_key(&candidate_type),
            Error::<T>::ReservedMissing
        );
        ensure!(Self::get_reserved(&candidate_type) == tx_id, Error::<T>::ReservedMismatch);

        // Ensure the signatures count satisfy quorum before accepting
        let quorum = signatures.len() as u32;
        ensure!(
            quorum >= AVN::<T>::calculate_two_third_quorum(),
            Error::<T>::NotEnoughConfirmations
        );

        // The following check is to ensure that we will not overwrite a value in the map,
        // this should never occur unless get_unique_transaction_identifier has a bug
        ensure!(!<Repository<T>>::contains_key(tx_id), Error::<T>::TransactionExists);

        let mut candidate_transaction = EthTransactionCandidate::new(
            tx_id,
            Some(T::AccountToBytesConvert::into_bytes(&submitter)),
            candidate_type,
            quorum,
        );

        for signature in signatures {
            let result = candidate_transaction.signatures.add(signature);
            if result.is_err() {
                log::error!("❌ Error while submitting signatures to ethereum-transactions pallet:{:?} {:?}", candidate_transaction, result);
                Err(Error::<T>::InvalidConfirmations)?
            }
        }

        <Repository<T>>::insert(candidate_transaction.tx_id, candidate_transaction.clone());
        <ReservedTransactions<T>>::remove(&candidate_transaction.call_data);

        Self::promote_candidate_transaction_to_dispatched(submitter, candidate_transaction.tx_id);
        Ok(())
    }

    #[cfg(feature = "runtime-benchmarks")]
    fn set_transaction_id(candidate_type: &EthTransactionType, id: TransactionId) {
        <ReservedTransactions<T>>::insert(candidate_type, id);
    }
}

#[derive(Encode, Decode, Default, Clone, Copy, PartialEq, Debug, Eq, MaxEncodedLen, TypeInfo)]
pub struct DispatchedData<BlockNumber: Member + AtLeast32Bit> {
    pub transaction_id: TransactionId,
    pub submitted_at_block: BlockNumber,
}

impl<BlockNumber: Member + AtLeast32Bit> DispatchedData<BlockNumber> {
    fn new(transaction_id: TransactionId, submitted_at_block: BlockNumber) -> Self {
        return DispatchedData::<BlockNumber> { transaction_id, submitted_at_block }
    }
}
