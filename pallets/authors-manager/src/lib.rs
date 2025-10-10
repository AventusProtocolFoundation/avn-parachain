//! # Authors manager Pallet
//!
//! This pallet provides functionality to add/remove authors.
//!
//! The pallet is based on the Substrate session pallet and implements related traits for session
//! management when authors are added or removed.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::string::String;

use sp_avn_common::eth::EthereumId;

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
    dispatch::DispatchResult,
    ensure,
    pallet_prelude::{DispatchClass, Weight},
    traits::Get,
    transactional,
};
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_session::{self as session, Config as SessionConfig};
use sp_core::{bounded::BoundedVec, ecdsa};
use sp_runtime::{
    scale_info::TypeInfo,
    traits::{Convert, Member, Saturating},
    DispatchError,
};
use sp_std::prelude::*;

use pallet_avn::{
    self as avn, AccountToBytesConverter, BridgeInterface, BridgeInterfaceNotification,
    NewSessionHandler, ValidatorRegistrationNotifier as AuthorRegistrationNotifier,
};
pub use sp_avn_common::{
    bounds::MaximumValidatorsBound as MaximumAuthorsBound,
    eth_key_actions::decompress_eth_public_key, event_types::Validator as Author, IngressCounter,
};

pub use pallet::*;

const PALLET_ID: &'static [u8; 14] = b"author_manager";

#[derive(Encode, Decode, Clone, PartialEq, Debug, TypeInfo, MaxEncodedLen)]
pub struct PendingAuthorRegistrationData<AccountId, BlockNumber> {
    pub account_id: AccountId,
    pub eth_public_key: ecdsa::Public,
    pub tx_id: EthereumId,
    pub timestamp: BlockNumber,
}

#[derive(Encode, Decode, Clone, PartialEq, Debug, TypeInfo, MaxEncodedLen)]
pub struct PendingAuthorDeregistrationData<BlockNumber> {
    pub tx_id: EthereumId,
    pub timestamp: BlockNumber,
    pub reason: AuthorDeregistrationReason,
}

#[derive(Encode, Decode, Copy, Clone, PartialEq, Debug, TypeInfo, MaxEncodedLen)]
pub enum PendingAuthorOperationType {
    Registration,
    Deregistration,
}

#[derive(Encode, Decode, Clone, PartialEq, Debug, TypeInfo)]
pub enum AuthorStatus {
    NotAuthor,
    PendingRegistration,
    Active,
    PendingDeregistration,
}

#[derive(Encode, Decode, Clone, PartialEq, Debug, TypeInfo, MaxEncodedLen)]
pub enum AuthorDeregistrationReason {
    Voluntary,
    Slashing,
    Governance,
}

pub mod default_weights;
pub use default_weights::WeightInfo;

pub type AVN<T> = avn::Pallet<T>;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::{assert_ok, pallet_prelude::*};
    use frame_system::{offchain::SendTransactionTypes, pallet_prelude::*};
    pub use pallet_avn::{EthereumPublicKeyChecker, MAX_VALIDATOR_ACCOUNTS as MAX_AUTHOR_ACCOUNTS};
    use sp_core::ecdsa;

    #[pallet::pallet]
    pub struct Pallet<T>(PhantomData<T>);

    #[pallet::config]
    pub trait Config:
        SendTransactionTypes<Call<Self>>
        + frame_system::Config
        + session::Config
        + pallet_avn::Config
        + pallet_session::historical::Config
    {
        /// Overarching event type
        type RuntimeEvent: From<Event<Self>>
            + Into<<Self as frame_system::Config>::RuntimeEvent>
            + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        type AccountToBytesConvert: AccountToBytesConverter<Self::AccountId>;

        type ValidatorRegistrationNotifier: AuthorRegistrationNotifier<
            <Self as session::Config>::ValidatorId,
        >;

        type WeightInfo: WeightInfo;

        type BridgeInterface: BridgeInterface;

        /// Maximum blocks a pending operation can remain before timeout
        #[pallet::constant]
        type PendingOperationTimeout: Get<BlockNumberFor<Self>>;

        /// Minimum number of authors that must remain active
        #[pallet::constant]
        type MinimumAuthorCount: Get<u32>;
    }

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// A new author has been registered. \[author_id, eth_key\]
        AuthorRegistered {
            author_id: T::AccountId,
            eth_key: ecdsa::Public,
        },
        /// An author has been deregistered. \[author_id\]
        AuthorDeregistered {
            author_id: T::AccountId,
        },
        /// An author has activation has started. \[author_id\]
        AuthorActivationStarted {
            author_id: T::AccountId,
        },
        /// An author action has been confirmed. \[action_id\]
        AuthorActionConfirmed {
            action_id: ActionId<T::AccountId>,
        },
        /// Failed to publish author action on Tier1. \[tx_id\]
        PublishingAuthorActionOnEthereumFailed {
            tx_id: u32,
        },
        /// Author action published on Tier1. \[tx_id\]
        PublishingAuthorActionOnEthereumSucceeded {
            tx_id: u32,
        },
        /// Author registration is pending T1 confirmation
        AuthorRegistrationPending {
            author_id: T::AccountId,
            eth_key: ecdsa::Public,
            tx_id: u32,
        },
        /// Author registration failed on T1
        AuthorRegistrationFailed {
            author_id: T::AccountId,
            tx_id: u32,
        },
        /// Author deregistration is pending T1 confirmation
        AuthorDeregistrationPending {
            author_id: T::AccountId,
            tx_id: u32,
        },
        /// Author deregistration failed on T1
        AuthorDeregistrationFailed {
            author_id: T::AccountId,
            tx_id: u32,
        },
        AuthorRegistrationTimedOut {
            author_id: T::AccountId,
            tx_id: u32,
        },
        AuthorDeregistrationTimedOut {
            author_id: T::AccountId,
            tx_id: u32,
        },
        AuthorOperationCancelled {
            author_id: T::AccountId,
            operation_type: PendingAuthorOperationType,
        },
        UnknownTransactionCallback {
            tx_id: u32,
        },
        T1TransactionSent {
            author_id: T::AccountId,
            tx_id: u32,
            operation_type: PendingAuthorOperationType,
        },
        OperationForcedComplete {
            author_id: T::AccountId,
            operation_type: PendingAuthorOperationType,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        /// There is no Tier1 event for adding authors
        NoTier1EventForAddingAuthor,
        /// There is no Tier1 event for removing authors
        NoTier1EventForRemovingAuthor,
        /// There are no authors in the chain
        NoAuthors,
        /// Author already exists
        AuthorAlreadyExists,
        /// The ingress counter is not valid
        InvalidIngressCounter,
        /// The minimum number of authors has been reached
        MinimumAuthorsReached,
        /// There was an nerror ending the voting period
        ErrorEndingVotingPeriod,
        /// The voting session is not valid
        VotingSessionIsNotValid,
        /// There was an error submitting transaction to Tier1
        ErrorSubmitCandidateTxnToTier1,
        /// There was an error calculating the primary author
        ErrorCalculatingPrimaryAuthor,
        /// Not action data found for author
        AuthorsActionDataNotFound,
        /// Removal already requested
        RemovalAlreadyRequested,
        /// There was an error converting accountId to AuthorId
        ErrorConvertingAccountIdToAuthorId,
        /// Slashed author is not found
        SlashedAuthorIsNotFound,
        /// Author not found
        AuthorNotFound,
        /// Invalid public key
        InvalidPublicKey,
        /// The ethereum public key of this author already exists
        AuthorEthKeyAlreadyExists,
        /// There was an error removing account from authors
        ErrorRemovingAccountFromAuthors,
        /// The maximum number of authors has been reached
        MaximumAuthorsReached,
        /// Transaction not found
        TransactionNotFound,
        /// Invalid action status for transaction
        InvalidActionStatus,
        /// Pending registration not found
        PendingRegistrationNotFound,
        /// Pending deregistration not found
        PendingDeregistrationNotFound,
        /// Unknown transaction ID
        UnknownTransaction,
        /// Pending operation already exists for this account
        PendingOperationExists,
        /// T1 transaction failed
        T1TransactionFailed,
        /// Operation has timed out
        OperationTimeout,
        RegistrationInProgress,
        DeregistrationInProgress,
        BridgePublishFailed,
        NoPendingOperationFound,
        /// Operation was already processed
        AlreadyProcessed,
    }

    #[pallet::storage]
    #[pallet::getter(fn author_account_ids)]
    pub type AuthorAccountIds<T: Config> =
        StorageValue<_, BoundedVec<T::AccountId, MaximumAuthorsBound>>;

    #[pallet::storage]
    pub type AuthorActions<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Blake2_128Concat,
        IngressCounter,
        AuthorsActionData,
        OptionQuery,
        GetDefault,
    >;

    #[pallet::storage]
    #[pallet::getter(fn get_author_by_eth_public_key)]
    pub type EthereumPublicKeys<T: Config> =
        StorageMap<_, Blake2_128Concat, ecdsa::Public, T::AccountId>;

    #[pallet::storage]
    #[pallet::getter(fn get_ingress_counter)]
    pub type TotalIngresses<T: Config> = StorageValue<_, IngressCounter, ValueQuery>;

    #[pallet::storage]
    pub type TransactionToAction<T: Config> =
        StorageMap<_, Blake2_128Concat, EthereumId, (T::AccountId, IngressCounter), OptionQuery>;

    /// Pending author registration requests awaiting T1 confirmation
    #[pallet::storage]
    pub type PendingAuthorRegistrations<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        PendingAuthorRegistrationData<T::AccountId, BlockNumberFor<T>>,
        OptionQuery,
    >;

    /// Pending author deregistration requests awaiting T1 confirmation
    #[pallet::storage]
    pub type PendingAuthorDeregistrations<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        PendingAuthorDeregistrationData<BlockNumberFor<T>>,
        OptionQuery,
    >;

    /// Transaction ID to Account ID mapping for pending author operations
    #[pallet::storage]
    pub type PendingAuthorTransactions<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        EthereumId,
        (T::AccountId, PendingAuthorOperationType),
        OptionQuery,
    >;

    /// Transactions that have been processed (to prevent replay)
    #[pallet::storage]
    pub type ProcessedTransactions<T: Config> =
        StorageMap<_, Blake2_128Concat, EthereumId, BlockNumberFor<T>, OptionQuery>;

    #[pallet::genesis_config]
    pub struct GenesisConfig<T: Config> {
        pub authors: Vec<(T::AccountId, ecdsa::Public)>,
    }

    impl<T: Config> Default for GenesisConfig<T> {
        fn default() -> Self {
            Self { authors: Vec::<(T::AccountId, ecdsa::Public)>::new() }
        }
    }

    #[pallet::genesis_build]
    impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
        fn build(&self) {
            log::debug!(
                "Authors Manager Genesis build entrypoint - total authors: {}",
                self.authors.len()
            );
            for (author_account_id, eth_public_key) in &self.authors {
                assert_ok!(<AuthorAccountIds<T>>::try_append(author_account_id));
                <EthereumPublicKeys<T>>::insert(eth_public_key, author_account_id);
            }
        }
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_initialize(n: BlockNumberFor<T>) -> Weight {
            // Calculate maximum weight budget (10% of max block weight)
            let max_weight = T::BlockWeights::get()
                .get(DispatchClass::Normal)
                .max_total
                .unwrap_or(Weight::from_parts(2_000_000_000_000, 0))
                .saturating_div(10);

            let mut remaining_weight = max_weight;

            // Check for timed out registrations
            if remaining_weight.ref_time() > 0 {
                let used = Self::cleanup_timed_out_registrations(n, remaining_weight);
                remaining_weight = remaining_weight.saturating_sub(used);
            }

            // Check for timed out deregistrations
            if remaining_weight.ref_time() > 0 {
                let used = Self::cleanup_timed_out_deregistrations(n, remaining_weight);
                remaining_weight = remaining_weight.saturating_sub(used);
            }

            // Cleanup old processed transactions
            if remaining_weight.ref_time() > 0 {
                let used = Self::cleanup_old_processed_transactions(n, remaining_weight);
                remaining_weight = remaining_weight.saturating_sub(used);
            }

            max_weight.saturating_sub(remaining_weight)
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Add a new author.
        /// This will send a T1 transaction first and wait for confirmation before making local
        /// changes.
        #[pallet::call_index(0)]
        #[pallet::weight(<T as Config>::WeightInfo::add_author())]
        #[transactional]
        pub fn add_author(
            origin: OriginFor<T>,
            author_account_id: T::AccountId,
            author_eth_public_key: ecdsa::Public,
        ) -> DispatchResult {
            ensure_root(origin)?;

            // Validate the registration request
            Self::validate_author_registration_request(&author_account_id, &author_eth_public_key)?;

            // Send T1 transaction FIRST to get tx_id
            let tx_id =
                Self::send_author_registration_to_t1(&author_account_id, &author_eth_public_key)?;

            // Store pending state with actual tx_id
            Self::store_pending_author_registration(
                &author_account_id,
                &author_eth_public_key,
                tx_id,
            )?;

            // Emit pending event
            Self::deposit_event(Event::<T>::AuthorRegistrationPending {
                author_id: author_account_id.clone(),
                eth_key: author_eth_public_key,
                tx_id,
            });

            Self::deposit_event(Event::<T>::T1TransactionSent {
                author_id: author_account_id,
                tx_id,
                operation_type: PendingAuthorOperationType::Registration,
            });

            Ok(())
        }

        /// Remove an author.
        /// This will send a T1 transaction first and wait for confirmation before making local
        /// changes.
        #[pallet::call_index(1)]
        #[pallet::weight(<T as Config>::WeightInfo::remove_author(MAX_AUTHOR_ACCOUNTS))]
        #[transactional]
        pub fn remove_author(
            origin: OriginFor<T>,
            author_account_id: T::AccountId,
        ) -> DispatchResult {
            ensure_root(origin)?;

            // Validate the deregistration request
            Self::validate_author_deregistration_request(&author_account_id)?;

            // Send T1 transaction FIRST to get tx_id
            let tx_id = Self::send_author_deregistration_to_t1(&author_account_id)?;

            // Store pending state with actual tx_id
            Self::store_pending_author_deregistration(
                &author_account_id,
                tx_id,
                AuthorDeregistrationReason::Voluntary,
            )?;

            // Emit pending event
            Self::deposit_event(Event::<T>::AuthorDeregistrationPending {
                author_id: author_account_id.clone(),
                tx_id,
            });

            Self::deposit_event(Event::<T>::T1TransactionSent {
                author_id: author_account_id,
                tx_id,
                operation_type: PendingAuthorOperationType::Deregistration,
            });

            Ok(())
        }

        #[pallet::call_index(2)]
        #[pallet::weight(<T as Config>::WeightInfo::rotate_author_ethereum_key())]
        #[transactional]
        pub fn rotate_author_ethereum_key(
            origin: OriginFor<T>,
            author_account_id: T::AccountId,
            author_old_eth_public_key: ecdsa::Public,
            author_new_eth_public_key: ecdsa::Public,
        ) -> DispatchResult {
            let _ = ensure_root(origin)?;

            ensure!(
                !<EthereumPublicKeys<T>>::contains_key(&author_new_eth_public_key),
                Error::<T>::AuthorEthKeyAlreadyExists
            );
            ensure!(
                author_old_eth_public_key != author_new_eth_public_key,
                Error::<T>::AuthorEthKeyAlreadyExists
            );

            let author_id = EthereumPublicKeys::<T>::take(&author_old_eth_public_key)
                .ok_or(Error::<T>::AuthorNotFound)?;
            ensure!(author_id == author_account_id, Error::<T>::AuthorNotFound);

            EthereumPublicKeys::<T>::insert(author_new_eth_public_key, author_id);
            Ok(())
        }

        /// Force complete an author registration that succeeded on T1 but failed locally.
        #[pallet::call_index(3)]
        #[pallet::weight(<T as Config>::WeightInfo::force_complete_registration())]
        #[transactional]
        pub fn force_complete_registration(
            origin: OriginFor<T>,
            account_id: T::AccountId,
        ) -> DispatchResult {
            ensure_root(origin)?;

            let pending_data = PendingAuthorRegistrations::<T>::get(&account_id)
                .ok_or(Error::<T>::PendingRegistrationNotFound)?;

            let author_account_ids = Self::author_account_ids().ok_or(Error::<T>::NoAuthors)?;
            ensure!(!author_account_ids.contains(&account_id), Error::<T>::AuthorAlreadyExists);

            log::warn!(
                "ADMIN ACTION: Force completing registration for {:?} (tx_id: {})",
                account_id,
                pending_data.tx_id
            );

            // Force complete (no staking, just add to authors list)
            Self::complete_author_registration(&account_id, &pending_data)?;

            // Cleanup pending state
            frame_support::storage::transactional::with_transaction(|| {
                PendingAuthorRegistrations::<T>::remove(&account_id);
                PendingAuthorTransactions::<T>::remove(pending_data.tx_id);
                frame_support::storage::TransactionOutcome::Commit(Ok::<(), DispatchError>(()))
            })?;

            Self::deposit_event(Event::<T>::OperationForcedComplete {
                author_id: account_id,
                operation_type: PendingAuthorOperationType::Registration,
            });

            Ok(())
        }

        /// Force complete an author deregistration that succeeded on T1 but failed locally.
        #[pallet::call_index(4)]
        #[pallet::weight(<T as Config>::WeightInfo::force_complete_deregistration())]
        #[transactional]
        pub fn force_complete_deregistration(
            origin: OriginFor<T>,
            account_id: T::AccountId,
        ) -> DispatchResult {
            ensure_root(origin)?;

            let pending_data = PendingAuthorDeregistrations::<T>::get(&account_id)
                .ok_or(Error::<T>::PendingDeregistrationNotFound)?;

            log::warn!(
                "ADMIN ACTION: Force completing deregistration for {:?} (tx_id: {})",
                account_id,
                pending_data.tx_id
            );

            // Force complete (no staking, just remove from authors list)
            Self::complete_author_deregistration(&account_id)?;

            // Cleanup pending state
            frame_support::storage::transactional::with_transaction(|| {
                PendingAuthorDeregistrations::<T>::remove(&account_id);
                PendingAuthorTransactions::<T>::remove(pending_data.tx_id);
                frame_support::storage::TransactionOutcome::Commit(Ok::<(), DispatchError>(()))
            })?;

            Self::deposit_event(Event::<T>::OperationForcedComplete {
                author_id: account_id,
                operation_type: PendingAuthorOperationType::Deregistration,
            });

            Ok(())
        }

        /// Cancel a pending author operation.
        #[pallet::call_index(5)]
        #[pallet::weight(<T as Config>::WeightInfo::cancel_pending_operation())]
        #[transactional]
        pub fn cancel_pending_operation(
            origin: OriginFor<T>,
            account_id: T::AccountId,
        ) -> DispatchResult {
            ensure_root(origin)?;

            log::warn!("ADMIN ACTION: Cancelling pending operation for {:?}", account_id);

            // Check for pending registration
            if let Some(pending_data) = PendingAuthorRegistrations::<T>::take(&account_id) {
                PendingAuthorTransactions::<T>::remove(pending_data.tx_id);

                Self::deposit_event(Event::<T>::AuthorOperationCancelled {
                    author_id: account_id,
                    operation_type: PendingAuthorOperationType::Registration,
                });

                return Ok(());
            }

            // Check for pending deregistration
            if let Some(pending_data) = PendingAuthorDeregistrations::<T>::take(&account_id) {
                PendingAuthorTransactions::<T>::remove(pending_data.tx_id);

                Self::deposit_event(Event::<T>::AuthorOperationCancelled {
                    author_id: account_id,
                    operation_type: PendingAuthorOperationType::Deregistration,
                });

                return Ok(());
            }

            Err(Error::<T>::NoPendingOperationFound.into())
        }
    }

    impl<T: Config> EthereumPublicKeyChecker<T::AccountId> for Pallet<T> {
        fn get_validator_for_eth_public_key(
            eth_public_key: &ecdsa::Public,
        ) -> Option<T::AccountId> {
            Self::get_author_by_eth_public_key(eth_public_key)
        }
    }
}

#[derive(Encode, Decode, Default, Clone, Copy, PartialEq, Debug, Eq, TypeInfo, MaxEncodedLen)]
pub struct ActionId<AccountId: Member> {
    pub action_account_id: AccountId,
    pub ingress_counter: IngressCounter,
}

#[derive(Copy, Clone, Eq, PartialEq, Encode, Decode, Debug, TypeInfo, MaxEncodedLen)]
pub enum AuthorsActionType {
    /// Author has asked to leave voluntarily
    Resignation,
    /// Author is being forced to leave due to a malicious behaviour
    Slashed,
    /// Author activates himself after he joins an active session
    Activation,
    /// Default value
    Unknown,
}

impl Default for AuthorsActionType {
    fn default() -> Self {
        AuthorsActionType::Unknown
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Encode, Decode, Debug, TypeInfo, MaxEncodedLen)]
pub enum AuthorsActionStatus {
    /// Author enters this state immediately within removal extrinsic, ready for session
    /// confirmation
    AwaitingConfirmation,
    /// Author enters this state within session handler, ready for signing and sending to T1
    Confirmed,
    /// Author enters this state once T1 action request is sent, ready to be removed from
    /// hashmap
    Actioned,
    /// Author enters this state once T1 event processed
    None,
}

impl Default for AuthorsActionStatus {
    fn default() -> Self {
        AuthorsActionStatus::None
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Encode, Decode, Debug, TypeInfo, MaxEncodedLen)]
pub struct AuthorsActionData {
    pub status: AuthorsActionStatus,
    pub eth_transaction_id: EthereumId,
    pub action_type: AuthorsActionType,
}

impl AuthorsActionData {
    fn new(
        status: AuthorsActionStatus,
        eth_transaction_id: EthereumId,
        action_type: AuthorsActionType,
    ) -> Self {
        return AuthorsActionData { status, eth_transaction_id, action_type }
    }
}

impl AuthorsActionType {
    fn is_deregistration(&self) -> bool {
        match self {
            AuthorsActionType::Resignation => true,
            AuthorsActionType::Slashed => true,
            _ => false,
        }
    }

    fn is_activation(&self) -> bool {
        match self {
            AuthorsActionType::Activation => true,
            _ => false,
        }
    }
}

impl<AccountId: Member + Encode> ActionId<AccountId> {
    fn new(action_account_id: AccountId, ingress_counter: IngressCounter) -> Self {
        return ActionId::<AccountId> { action_account_id, ingress_counter }
    }
}

impl<T: Config> Pallet<T> {
    // Maximum cleanup items per block to prevent weight overflow
    const MAX_TIMEOUT_CLEANUPS_PER_BLOCK: usize = 10;
    const MAX_PROCESSED_TX_CLEANUPS_PER_BLOCK: usize = 50;

    fn cleanup_timed_out_registrations(
        current_block: BlockNumberFor<T>,
        max_weight: Weight,
    ) -> Weight {
        let timeout_threshold = T::PendingOperationTimeout::get();
        let mut weight = T::DbWeight::get().reads(1);

        if weight.ref_time() >= max_weight.ref_time() {
            return weight;
        }

        let mut timed_out = Vec::with_capacity(Self::MAX_TIMEOUT_CLEANUPS_PER_BLOCK);
        let mut iterations = 0u32;
        const MAX_ITERATIONS: u32 = 100;

        for (account_id, pending_data) in PendingAuthorRegistrations::<T>::iter() {
            iterations += 1;

            if iterations >= MAX_ITERATIONS {
                if timed_out.is_empty() {
                    log::warn!(
                        "Reached iteration limit in cleanup_timed_out_registrations \
                        without finding any timed out operations"
                    );
                }
                break;
            }

            if current_block.saturating_sub(pending_data.timestamp) > timeout_threshold {
                timed_out.push(account_id);
                if timed_out.len() >= Self::MAX_TIMEOUT_CLEANUPS_PER_BLOCK {
                    break;
                }
            }
        }

        weight = weight.saturating_add(T::DbWeight::get().reads(iterations as u64));

        for account_id in timed_out {
            let operation_weight = T::DbWeight::get().reads_writes(1, 2);
            if weight.saturating_add(operation_weight).ref_time() > max_weight.ref_time() {
                break;
            }

            if let Some(pending_data) = PendingAuthorRegistrations::<T>::take(&account_id) {
                PendingAuthorTransactions::<T>::remove(pending_data.tx_id);

                Self::deposit_event(Event::<T>::AuthorRegistrationTimedOut {
                    author_id: account_id,
                    tx_id: pending_data.tx_id,
                });

                weight = weight.saturating_add(operation_weight);
            }
        }

        weight
    }

    fn cleanup_timed_out_deregistrations(
        current_block: BlockNumberFor<T>,
        max_weight: Weight,
    ) -> Weight {
        let timeout_threshold = T::PendingOperationTimeout::get();
        let mut weight = T::DbWeight::get().reads(1);

        if weight.ref_time() >= max_weight.ref_time() {
            return weight;
        }

        let mut timed_out = Vec::with_capacity(Self::MAX_TIMEOUT_CLEANUPS_PER_BLOCK);
        let mut iterations = 0u32;
        const MAX_ITERATIONS: u32 = 100;

        for (account_id, pending_data) in PendingAuthorDeregistrations::<T>::iter() {
            iterations += 1;

            if iterations >= MAX_ITERATIONS {
                if timed_out.is_empty() {
                    log::warn!(
                        "Reached iteration limit in cleanup_timed_out_deregistrations \
                        without finding any timed out operations"
                    );
                }
                break;
            }

            if current_block.saturating_sub(pending_data.timestamp) > timeout_threshold {
                timed_out.push(account_id);
                if timed_out.len() >= Self::MAX_TIMEOUT_CLEANUPS_PER_BLOCK {
                    break;
                }
            }
        }

        weight = weight.saturating_add(T::DbWeight::get().reads(iterations as u64));

        for account_id in timed_out {
            let operation_weight = T::DbWeight::get().reads_writes(1, 2);
            if weight.saturating_add(operation_weight).ref_time() > max_weight.ref_time() {
                break;
            }

            if let Some(pending_data) = PendingAuthorDeregistrations::<T>::take(&account_id) {
                PendingAuthorTransactions::<T>::remove(pending_data.tx_id);

                Self::deposit_event(Event::<T>::AuthorDeregistrationTimedOut {
                    author_id: account_id,
                    tx_id: pending_data.tx_id,
                });

                weight = weight.saturating_add(operation_weight);
            }
        }

        weight
    }

    fn cleanup_old_processed_transactions(
        current_block: BlockNumberFor<T>,
        max_weight: Weight,
    ) -> Weight {
        const RETENTION_PERIOD: u32 = 28800; // 48 hours at 6s/block
        let cutoff_block = current_block.saturating_sub(RETENTION_PERIOD.into());

        let mut weight = T::DbWeight::get().reads(1);

        if weight.ref_time() >= max_weight.ref_time() {
            return weight;
        }

        let mut to_remove = Vec::with_capacity(Self::MAX_PROCESSED_TX_CLEANUPS_PER_BLOCK);
        let mut iterations = 0u32;
        const MAX_ITERATIONS: u32 = 200;

        for (tx_id, processed_at) in ProcessedTransactions::<T>::iter() {
            iterations += 1;

            if iterations >= MAX_ITERATIONS {
                if to_remove.is_empty() {
                    log::warn!(
                        "Reached iteration limit in cleanup_old_processed_transactions \
                        without finding any old transactions"
                    );
                }
                break;
            }

            if processed_at < cutoff_block {
                to_remove.push(tx_id);
                if to_remove.len() >= Self::MAX_PROCESSED_TX_CLEANUPS_PER_BLOCK {
                    break;
                }
            }
        }

        weight = weight.saturating_add(T::DbWeight::get().reads(iterations as u64));

        for tx_id in to_remove {
            let operation_weight = T::DbWeight::get().writes(1);
            if weight.saturating_add(operation_weight).ref_time() > max_weight.ref_time() {
                break;
            }

            ProcessedTransactions::<T>::remove(tx_id);
            weight = weight.saturating_add(operation_weight);
        }

        weight
    }

    fn start_activation_for_registered_author(registered_author: &T::AccountId, tx_id: EthereumId) {
        let ingress_counter = Self::get_ingress_counter() + 1;

        TotalIngresses::<T>::put(ingress_counter);
        <AuthorActions<T>>::insert(
            registered_author,
            ingress_counter,
            AuthorsActionData::new(
                AuthorsActionStatus::AwaitingConfirmation,
                tx_id,
                AuthorsActionType::Activation,
            ),
        );
    }

    // Validation functions

    fn validate_author_registration_request(
        author_account_id: &T::AccountId,
        author_eth_public_key: &ecdsa::Public,
    ) -> DispatchResult {
        // Check for conflicting operations
        Self::validate_no_conflicting_operations(author_account_id)?;

        let author_account_ids = Self::author_account_ids().ok_or(Error::<T>::NoAuthors)?;
        ensure!(!author_account_ids.is_empty(), Error::<T>::NoAuthors);

        ensure!(!author_account_ids.contains(author_account_id), Error::<T>::AuthorAlreadyExists);

        ensure!(
            !<EthereumPublicKeys<T>>::contains_key(author_eth_public_key),
            Error::<T>::AuthorEthKeyAlreadyExists
        );

        ensure!(
            author_account_ids.len() < (<MaximumAuthorsBound as sp_core::TypedGet>::get() as usize),
            Error::<T>::MaximumAuthorsReached
        );

        Ok(())
    }

    fn validate_author_deregistration_request(account_id: &T::AccountId) -> DispatchResult {
        // Check for conflicting operations
        Self::validate_no_conflicting_operations(account_id)?;

        let author_account_ids = Self::author_account_ids().ok_or(Error::<T>::NoAuthors)?;

        ensure!(
            author_account_ids.len() > T::MinimumAuthorCount::get() as usize,
            Error::<T>::MinimumAuthorsReached
        );

        ensure!(author_account_ids.contains(account_id), Error::<T>::AuthorNotFound);

        Ok(())
    }

    fn validate_no_conflicting_operations(account_id: &T::AccountId) -> DispatchResult {
        ensure!(
            !PendingAuthorRegistrations::<T>::contains_key(account_id),
            Error::<T>::RegistrationInProgress
        );

        ensure!(
            !PendingAuthorDeregistrations::<T>::contains_key(account_id),
            Error::<T>::DeregistrationInProgress
        );

        Ok(())
    }

    // T1 Communication functions

    fn send_author_registration_to_t1(
        author_account_id: &T::AccountId,
        author_eth_public_key: &ecdsa::Public,
    ) -> Result<EthereumId, DispatchError> {
        let decompressed_eth_public_key = decompress_eth_public_key(*author_eth_public_key)
            .map_err(|_| Error::<T>::InvalidPublicKey)?;

        let author_id_bytes =
            <T as pallet::Config>::AccountToBytesConvert::into_bytes(author_account_id);

        let params = vec![
            (b"bytes".to_vec(), decompressed_eth_public_key.to_fixed_bytes().to_vec()),
            (b"bytes32".to_vec(), author_id_bytes.to_vec()),
        ];

        <T as pallet::Config>::BridgeInterface::publish(b"addAuthor", &params, PALLET_ID.to_vec())
            .map_err(|_| Error::<T>::BridgePublishFailed.into())
    }

    fn send_author_deregistration_to_t1(
        author_account_id: &T::AccountId,
    ) -> Result<EthereumId, DispatchError> {
        let eth_public_key = Self::get_ethereum_public_key_if_exists(author_account_id)
            .ok_or(Error::<T>::AuthorNotFound)?;

        let decompressed_eth_public_key =
            decompress_eth_public_key(eth_public_key).map_err(|_| Error::<T>::InvalidPublicKey)?;

        let author_id_bytes =
            <T as pallet::Config>::AccountToBytesConvert::into_bytes(author_account_id);

        let params = vec![
            (b"bytes32".to_vec(), author_id_bytes.to_vec()),
            (b"bytes".to_vec(), decompressed_eth_public_key.to_fixed_bytes().to_vec()),
        ];

        <T as pallet::Config>::BridgeInterface::publish(
            b"removeAuthor",
            &params,
            PALLET_ID.to_vec(),
        )
        .map_err(|_| Error::<T>::BridgePublishFailed.into())
    }

    // Storage functions

    fn store_pending_author_registration(
        account_id: &T::AccountId,
        eth_public_key: &ecdsa::Public,
        tx_id: EthereumId,
    ) -> DispatchResult {
        let current_block = <frame_system::Pallet<T>>::block_number();
        let pending_data = PendingAuthorRegistrationData {
            account_id: account_id.clone(),
            eth_public_key: *eth_public_key,
            tx_id,
            timestamp: current_block,
        };

        frame_support::storage::transactional::with_transaction(|| {
            PendingAuthorRegistrations::<T>::insert(account_id, pending_data);
            PendingAuthorTransactions::<T>::insert(
                tx_id,
                (account_id.clone(), PendingAuthorOperationType::Registration),
            );

            frame_support::storage::TransactionOutcome::Commit(Ok::<(), DispatchError>(()))
        })?;

        Ok(())
    }

    fn store_pending_author_deregistration(
        account_id: &T::AccountId,
        tx_id: EthereumId,
        reason: AuthorDeregistrationReason,
    ) -> DispatchResult {
        let current_block = <frame_system::Pallet<T>>::block_number();
        let pending_data =
            PendingAuthorDeregistrationData { tx_id, timestamp: current_block, reason };

        frame_support::storage::transactional::with_transaction(|| {
            PendingAuthorDeregistrations::<T>::insert(account_id, pending_data);
            PendingAuthorTransactions::<T>::insert(
                tx_id,
                (account_id.clone(), PendingAuthorOperationType::Deregistration),
            );

            frame_support::storage::TransactionOutcome::Commit(Ok::<(), DispatchError>(()))
        })?;

        Ok(())
    }

    // Completion functions (no staking - just add/remove from list)

    fn complete_author_registration(
        account_id: &T::AccountId,
        pending_data: &PendingAuthorRegistrationData<T::AccountId, BlockNumberFor<T>>,
    ) -> DispatchResult {
        // Add to active authors list (NO staking)
        <AuthorAccountIds<T>>::try_append(account_id.clone())
            .map_err(|_| Error::<T>::MaximumAuthorsReached)?;

        // Add ethereum key mapping
        <EthereumPublicKeys<T>>::insert(pending_data.eth_public_key, account_id);

        // Notify author registration
        let new_author_id = <T as SessionConfig>::ValidatorIdOf::convert(account_id.clone())
            .ok_or(Error::<T>::ErrorConvertingAccountIdToAuthorId)?;
        T::ValidatorRegistrationNotifier::on_validator_registration(&new_author_id);

        // Start activation process
        Self::start_activation_for_registered_author(account_id, pending_data.tx_id);

        // Emit success event
        Self::deposit_event(Event::<T>::AuthorRegistered {
            author_id: account_id.clone(),
            eth_key: pending_data.eth_public_key,
        });

        Ok(())
    }

    fn complete_author_deregistration(account_id: &T::AccountId) -> DispatchResult {
        // Remove from author list (NO staking - simple removal)
        if let Some(mut author_account_ids) = Self::author_account_ids() {
            if let Some(index) = author_account_ids.iter().position(|v| v == account_id) {
                author_account_ids.swap_remove(index);
                <AuthorAccountIds<T>>::put(author_account_ids);
            }
        }

        // Remove ethereum key mapping
        Self::remove_ethereum_public_key_if_required(account_id);

        // Emit event
        Self::deposit_event(Event::<T>::AuthorDeregistered { author_id: account_id.clone() });

        Ok(())
    }

    // Callback handlers

    fn handle_author_registration_result(
        account_id: T::AccountId,
        tx_id: EthereumId,
        succeeded: bool,
    ) -> DispatchResult {
        let pending_data = PendingAuthorRegistrations::<T>::get(&account_id)
            .ok_or(Error::<T>::PendingRegistrationNotFound)?;

        if succeeded {
            Self::complete_author_registration(&account_id, &pending_data)?;
        } else {
            Self::deposit_event(Event::<T>::AuthorRegistrationFailed {
                author_id: account_id.clone(),
                tx_id,
            });
        }

        frame_support::storage::transactional::with_transaction(|| {
            PendingAuthorRegistrations::<T>::remove(&account_id);
            PendingAuthorTransactions::<T>::remove(tx_id);

            frame_support::storage::TransactionOutcome::Commit(Ok::<(), DispatchError>(()))
        })?;

        Ok(())
    }

    fn handle_author_deregistration_result(
        account_id: T::AccountId,
        tx_id: EthereumId,
        succeeded: bool,
    ) -> DispatchResult {
        let _pending_data = PendingAuthorDeregistrations::<T>::get(&account_id)
            .ok_or(Error::<T>::PendingDeregistrationNotFound)?;

        if succeeded {
            Self::complete_author_deregistration(&account_id)?;
        } else {
            Self::deposit_event(Event::<T>::AuthorDeregistrationFailed {
                author_id: account_id.clone(),
                tx_id,
            });
        }

        frame_support::storage::transactional::with_transaction(|| {
            PendingAuthorDeregistrations::<T>::remove(&account_id);
            PendingAuthorTransactions::<T>::remove(tx_id);

            frame_support::storage::TransactionOutcome::Commit(Ok::<(), DispatchError>(()))
        })?;

        Ok(())
    }

    fn get_ethereum_public_key_if_exists(account_id: &T::AccountId) -> Option<ecdsa::Public> {
        return <EthereumPublicKeys<T>>::iter()
            .filter(|(_, acc)| acc == account_id)
            .map(|(pk, _)| pk)
            .nth(0)
    }

    fn remove_ethereum_public_key_if_required(author_id: &T::AccountId) {
        let public_key_to_remove = Self::get_ethereum_public_key_if_exists(&author_id);
        if let Some(public_key_to_remove) = public_key_to_remove {
            <EthereumPublicKeys<T>>::remove(public_key_to_remove);
        }
    }

    // Legacy remove functions removed - use new async flow instead

    fn author_permanently_removed(
        active_authors: &Vec<Author<T::AuthorityId, T::AccountId>>,
        disabled_authors: &Vec<T::AccountId>,
        deregistered_author: &T::AccountId,
    ) -> bool {
        // If the author exists in either vectors then they have not been removed from the
        // session
        return !active_authors.iter().any(|v| &v.account_id == deregistered_author) &&
            !disabled_authors.iter().any(|v| v == deregistered_author)
    }

    fn clean_up_author_data(action_account_id: T::AccountId, ingress_counter: IngressCounter) {
        <AuthorActions<T>>::mutate(
            &action_account_id,
            ingress_counter,
            |authors_action_data_maybe| {
                if let Some(authors_action_data) = authors_action_data_maybe {
                    authors_action_data.status = AuthorsActionStatus::Confirmed
                }
            },
        );
        Self::remove_ethereum_public_key_if_required(&action_account_id);

        let action_id = ActionId::new(action_account_id, ingress_counter);

        Self::deposit_event(Event::<T>::AuthorActionConfirmed { action_id });
    }

    fn process_transaction(tx_id: EthereumId, succeeded: bool) -> Result<(), DispatchError> {
        let (account_id, ingress_counter) =
            TransactionToAction::<T>::get(tx_id).ok_or(Error::<T>::TransactionNotFound)?;

        let action_data = AuthorActions::<T>::get(&account_id, ingress_counter)
            .ok_or(Error::<T>::AuthorsActionDataNotFound)?;

        ensure!(
            action_data.status == AuthorsActionStatus::Confirmed,
            Error::<T>::InvalidActionStatus
        );

        if succeeded {
            AuthorActions::<T>::remove(&account_id, ingress_counter);
            TransactionToAction::<T>::remove(tx_id);

            Self::deposit_event(Event::<T>::PublishingAuthorActionOnEthereumSucceeded { tx_id });
        } else {
            Self::deposit_event(Event::<T>::PublishingAuthorActionOnEthereumFailed { tx_id });
        }

        Ok(())
    }
}

impl<T: Config> NewSessionHandler<T::AuthorityId, T::AccountId> for Pallet<T> {
    fn on_genesis_session(_authors: &Vec<Author<T::AuthorityId, T::AccountId>>) {
        log::trace!("Authors manager on_genesis_session");
    }

    fn on_new_session(
        _changed: bool,
        active_authors: &Vec<Author<T::AuthorityId, T::AccountId>>,
        disabled_authors: &Vec<T::AccountId>,
    ) {
        log::trace!("Authors manager on_new_session");
        if <AuthorActions<T>>::iter().count() > 0 {
            for (action_account_id, ingress_counter, authors_action_data) in
                <AuthorActions<T>>::iter()
            {
                if authors_action_data.status == AuthorsActionStatus::AwaitingConfirmation &&
                    authors_action_data.action_type.is_deregistration() &&
                    Self::author_permanently_removed(
                        &active_authors,
                        &disabled_authors,
                        &action_account_id,
                    )
                {
                    Self::clean_up_author_data(action_account_id, ingress_counter);
                } else if authors_action_data.status == AuthorsActionStatus::AwaitingConfirmation &&
                    authors_action_data.action_type.is_activation()
                {
                    <AuthorActions<T>>::mutate(
                        &action_account_id,
                        ingress_counter,
                        |authors_action_data_maybe| {
                            if let Some(authors_action_data) = authors_action_data_maybe {
                                authors_action_data.status = AuthorsActionStatus::Confirmed
                            }
                        },
                    );

                    Self::deposit_event(Event::<T>::AuthorActivationStarted {
                        author_id: action_account_id.clone(),
                    });
                }
            }
        }
    }
}

impl<T: Config> session::SessionManager<T::AccountId> for Pallet<T> {
    fn new_session(new_index: u32) -> Option<Vec<T::AccountId>> {
        // Retrieve the authors from storage
        let authors_option = AuthorAccountIds::<T>::get();

        if let Some(authors) = authors_option {
            if authors.is_empty() {
                // We never want to pass an empty set of authors. This would brick the chain.
                log::error!("💥 keeping old session because of empty author set!");
                None
            } else {
                log::debug!(
                    "[AUTH-MGR] assembling new authors for new session {} with these authors {:#?} at #{:?}",
                    new_index,
                    authors,
                    <frame_system::Pallet<T>>::block_number(),
                );

                Some(authors.into())
            }
        } else {
            // Handle the case where no authors are present in storage
            log::error!("💥 keeping old session because no authors found in storage!");
            None
        }
    }

    fn end_session(_end_index: u32) {
        // nothing to do here
    }

    fn start_session(_start_index: u32) {
        // nothing to do here
    }
}

impl<T: Config> BridgeInterfaceNotification for Pallet<T> {
    fn process_result(tx_id: u32, caller_id: Vec<u8>, succeeded: bool) -> DispatchResult {
        if caller_id != PALLET_ID.to_vec() {
            return Ok(());
        }

        // Reentrancy protection: check if already processed
        if ProcessedTransactions::<T>::contains_key(tx_id) {
            return Ok(()); // Silently ignore duplicates
        }

        // Mark as processed FIRST
        ProcessedTransactions::<T>::insert(tx_id, <frame_system::Pallet<T>>::block_number());

        // Check if this is a pending operation transaction
        if let Some((account_id, operation_type)) = PendingAuthorTransactions::<T>::get(tx_id) {
            let result = match operation_type {
                PendingAuthorOperationType::Registration =>
                    Self::handle_author_registration_result(account_id, tx_id, succeeded),
                PendingAuthorOperationType::Deregistration =>
                    Self::handle_author_deregistration_result(account_id, tx_id, succeeded),
            };

            // If processing failed, remove from processed set to allow retry
            if result.is_err() {
                ProcessedTransactions::<T>::remove(tx_id);
            }

            result?;
        } else {
            // Check if this is an old-style action transaction (for backwards compatibility)
            if let Some((_account_id, _ingress_counter)) = TransactionToAction::<T>::get(tx_id) {
                // Handle old-style transactions
                Self::process_transaction(tx_id, succeeded)?;
            } else {
                // Unknown transaction
                Self::deposit_event(Event::<T>::UnknownTransactionCallback { tx_id });
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

mod benchmarking;
