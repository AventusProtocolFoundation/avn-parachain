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
use frame_support::{dispatch::DispatchResult, ensure, traits::Get, transactional};
use pallet_session::{self as session, Config as SessionConfig};
use sp_core::{bounded::BoundedVec, ecdsa};
use sp_runtime::{
    scale_info::TypeInfo,
    traits::{Convert, Member},
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

pub const PALLET_ID: &'static [u8; 14] = b"author_manager";

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

        /// Minimum number of authors that must remain active
        #[pallet::constant]
        type MinimumAuthorsCount: Get<u32>;
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
            Self::validate_author_registration_request(
                &author_account_id,
                &author_eth_public_key,
            )?;

            // Send to T1 - actual registration happens in callback
            Self::send_author_registration_to_t1(
                &author_account_id,
                &author_eth_public_key,
            )?;

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

            // Send to T1 - actual deregistration happens in callback
            Self::send_author_deregistration_to_t1(&author_account_id)?;

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
    /// Author registration pending T1 confirmation
    Registration,
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

    fn is_registration(&self) -> bool {
        match self {
            AuthorsActionType::Registration => true,
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
    fn validate_author_registration_request(
        account_id: &T::AccountId,
        eth_public_key: &ecdsa::Public,
    ) -> DispatchResult {
        let author_account_ids =
            Self::author_account_ids().ok_or(Error::<T>::NoAuthors)?;
        ensure!(!author_account_ids.is_empty(), Error::<T>::NoAuthors);

        ensure!(!author_account_ids.contains(account_id), Error::<T>::AuthorAlreadyExists);

        ensure!(
            !<EthereumPublicKeys<T>>::contains_key(eth_public_key),
            Error::<T>::AuthorEthKeyAlreadyExists
        );

        ensure!(
            author_account_ids.len() <
                (<MaximumAuthorsBound as sp_core::TypedGet>::get() as usize),
            Error::<T>::MaximumAuthorsReached
        );

        Ok(())
    }

    fn validate_author_deregistration_request(account_id: &T::AccountId) -> DispatchResult {
        let author_account_ids =
            Self::author_account_ids().ok_or(Error::<T>::NoAuthors)?;

        ensure!(
            author_account_ids.len() > T::MinimumAuthorsCount::get() as usize,
            Error::<T>::MinimumAuthorsReached
        );

        ensure!(author_account_ids.contains(account_id), Error::<T>::AuthorNotFound);

        // Check for conflicting deregistration already in progress
        ensure!(
            !Self::has_active_deregistration(account_id),
            Error::<T>::RemovalAlreadyRequested
        );

        Ok(())
    }

    fn has_active_deregistration(author_account_id: &T::AccountId) -> bool {
        <AuthorActions<T>>::iter_prefix_values(author_account_id).any(
            |authors_action_data| {
                authors_action_data.action_type.is_deregistration() &&
                    Self::deregistration_state_is_active(authors_action_data.status)
            },
        )
    }

    fn deregistration_state_is_active(status: AuthorsActionStatus) -> bool {
        matches!(
            status,
            AuthorsActionStatus::AwaitingConfirmation | AuthorsActionStatus::Confirmed
        )
    }

    fn author_permanently_removed(
        active_authors: &Vec<Author<T::AuthorityId, T::AccountId>>,
        disabled_authors: &Vec<T::AccountId>,
        deregistered_author: &T::AccountId,
    ) -> bool {
        !active_authors.iter().any(|v| &v.account_id == deregistered_author) &&
            !disabled_authors.iter().any(|v| v == deregistered_author)
    }

    /// Send author registration request to T1
    fn send_author_registration_to_t1(
        author_account_id: &T::AccountId,
        author_eth_public_key: &ecdsa::Public,
    ) -> Result<EthereumId, DispatchError> {
        // Add eth key mapping immediately (before T1 confirmation)
        <EthereumPublicKeys<T>>::insert(author_eth_public_key, author_account_id);

        // Prepare data for T1
        let decompressed_eth_public_key = decompress_eth_public_key(*author_eth_public_key)
            .map_err(|_| Error::<T>::InvalidPublicKey)?;

        let author_id_bytes =
            <T as pallet::Config>::AccountToBytesConvert::into_bytes(author_account_id);

        let params = vec![
            (b"bytes".to_vec(), decompressed_eth_public_key.to_fixed_bytes().to_vec()),
            (b"bytes32".to_vec(), author_id_bytes.to_vec()),
        ];

        let tx_id = <T as pallet::Config>::BridgeInterface::publish(
            b"addAuthor",
            &params,
            PALLET_ID.to_vec(),
        )
        .map_err(|e| DispatchError::Other(e.into()))?;

        // Now create AuthorActions entry with the actual tx_id (single insert, no mutation)
        let ingress_counter = Self::get_ingress_counter() + 1;
        TotalIngresses::<T>::put(ingress_counter);

        <AuthorActions<T>>::insert(
            author_account_id,
            ingress_counter,
            AuthorsActionData::new(
                AuthorsActionStatus::AwaitingConfirmation,
                tx_id,
                AuthorsActionType::Registration,
            ),
        );

        Self::deposit_event(Event::<T>::PublishingAuthorActionOnEthereumSucceeded { tx_id });

        Ok(tx_id)
    }

    /// Send author deregistration request to T1
    fn send_author_deregistration_to_t1(
        author_account_id: &T::AccountId,
    ) -> Result<EthereumId, DispatchError> {
        // Prepare data for T1
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

        // Send to T1 and get tx_id FIRST
        let tx_id = <T as pallet::Config>::BridgeInterface::publish(
            b"removeAuthor",
            &params,
            PALLET_ID.to_vec(),
        )
        .map_err(|e| DispatchError::Other(e.into()))?;

        // Now create AuthorActions entry with the actual tx_id (single insert, no mutation)
        let ingress_counter = Self::get_ingress_counter() + 1;
        TotalIngresses::<T>::put(ingress_counter);

        <AuthorActions<T>>::insert(
            author_account_id,
            ingress_counter,
            AuthorsActionData::new(
                AuthorsActionStatus::AwaitingConfirmation,
                tx_id,
                AuthorsActionType::Resignation,
            ),
        );

        Self::deposit_event(Event::<T>::PublishingAuthorActionOnEthereumSucceeded { tx_id });

        Ok(tx_id)
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
                } else if authors_action_data.status ==
                    AuthorsActionStatus::AwaitingConfirmation &&
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
                } else if authors_action_data.status == AuthorsActionStatus::Confirmed &&
                    authors_action_data.action_type.is_activation()
                {
                    // Activation is complete - move to Actioned for cleanup
                    <AuthorActions<T>>::mutate(
                        &action_account_id,
                        ingress_counter,
                        |authors_action_data_maybe| {
                            if let Some(authors_action_data) = authors_action_data_maybe {
                                authors_action_data.status = AuthorsActionStatus::Actioned
                            }
                        },
                    );
                } else if authors_action_data.status == AuthorsActionStatus::Actioned {
                    // Remove completed actions to prevent storage bloat
                    <AuthorActions<T>>::remove(&action_account_id, ingress_counter);
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
            return Ok(())
        }

        // Find the AuthorActions entry with matching tx_id
        let mut found_entry: Option<(T::AccountId, IngressCounter, AuthorsActionType)> = None;
        
        for (account_id, ingress_counter, authors_action_data) in <AuthorActions<T>>::iter() {
            if authors_action_data.eth_transaction_id == tx_id {
                found_entry = Some((
                    account_id,
                    ingress_counter,
                    authors_action_data.action_type,
                ));
                break;
            }
        }

        let Some((account_id, ingress_counter, action_type)) = found_entry else {
            // No matching entry found - might have been cleaned up already
            return Ok(())
        };

        if !succeeded {
            // T1 operation failed - cleanup the action and rollback state
            if action_type.is_registration() {
                // Remove the eth key mapping we added optimistically
                if let Some(eth_key) = Self::get_ethereum_public_key_if_exists(&account_id) {
                    <EthereumPublicKeys<T>>::remove(eth_key);
                }
            }
            
            <AuthorActions<T>>::remove(&account_id, ingress_counter);
            Self::deposit_event(Event::<T>::PublishingAuthorActionOnEthereumFailed { tx_id });
            return Ok(())
        }

        // T1 succeeded - complete the operation based on action type
        if action_type.is_registration() {
            // Complete registration
            // Add to active authors list
            match <AuthorAccountIds<T>>::try_append(account_id.clone()) {
                Ok(_) => {},
                Err(_) => {
                    log::error!("Failed to append author to AuthorAccountIds");
                    // Cleanup on failure
                    if let Some(eth_key) = Self::get_ethereum_public_key_if_exists(&account_id) {
                        <EthereumPublicKeys<T>>::remove(eth_key);
                    }
                    <AuthorActions<T>>::remove(&account_id, ingress_counter);
                    return Err(Error::<T>::MaximumAuthorsReached.into())
                }
            }

            // Notify author registration
            let new_author_id =
                <T as SessionConfig>::ValidatorIdOf::convert(account_id.clone())
                    .ok_or(Error::<T>::ErrorConvertingAccountIdToAuthorId)?;
            T::ValidatorRegistrationNotifier::on_validator_registration(&new_author_id);

            // Get eth key for event (we know it exists because we added it earlier)
            let eth_key = Self::get_ethereum_public_key_if_exists(&account_id)
                .ok_or(Error::<T>::AuthorNotFound)?;

            // Update AuthorActions for activation process
            <AuthorActions<T>>::mutate(
                &account_id,
                ingress_counter,
                |authors_action_data_maybe| {
                    if let Some(authors_action_data) = authors_action_data_maybe {
                        authors_action_data.action_type = AuthorsActionType::Activation;
                    }
                },
            );

            // Emit success event
            Self::deposit_event(Event::<T>::AuthorRegistered {
                author_id: account_id,
                eth_key,
            });
        } else if action_type.is_deregistration() {
            // For deregistration, remove from author list immediately
            // Session handler will complete the cleanup via clean_up_author_data
            let mut author_account_ids = Self::author_account_ids().ok_or(Error::<T>::NoAuthors)?;
            
            if let Some(index) = author_account_ids.iter().position(|v| v == &account_id) {
                author_account_ids.swap_remove(index);
                <AuthorAccountIds<T>>::put(author_account_ids);
                
                Self::deposit_event(Event::<T>::AuthorDeregistered {
                    author_id: account_id,
                });
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
