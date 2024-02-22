#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::string::String;

use codec::{Decode, Encode, MaxEncodedLen};
use sp_avn_common::event_types::Validator;
use sp_std::prelude::*;

use frame_support::{
    dispatch::{DispatchError, DispatchResult},
    log,
};
use frame_system::{pallet_prelude::BlockNumberFor, offchain::SubmitTransaction};
use pallet_avn::{self as avn, vote::*, Error as avn_error};
use sp_application_crypto::RuntimeAppPublic;
use sp_runtime::scale_info::TypeInfo;
use sp_std::fmt::Debug;

use super::{Call, Config, VotesRepository, PendingApproval};
use crate::{OcwLock, Pallet as Summary, RootId, AVN, Store};

pub const CAST_VOTE_CONTEXT: &'static [u8] = b"root_casting_vote";
pub const END_VOTING_PERIOD_CONTEXT: &'static [u8] = b"root_end_voting_period";
const MAX_VOTING_SESSIONS_RETURNED: usize = 5;

#[derive(PartialEq, Eq, Clone, Encode, Decode, Default, Debug, MaxEncodedLen, TypeInfo)]
pub struct RootVotingSession<T: Config> {
    pub root_id: RootId<BlockNumberFor<T>>,
}

impl<T: Config> RootVotingSession<T> {
    pub fn new(root_id: &RootId<BlockNumberFor<T>>) -> Self {
        return RootVotingSession::<T> { root_id: root_id.clone() }
    }
}

impl<T: Config> VotingSessionManager<T::AccountId, BlockNumberFor<T>> for RootVotingSession<T> {
    fn cast_vote_context(&self) -> &'static [u8] {
        return CAST_VOTE_CONTEXT
    }

    fn end_voting_period_context(&self) -> &'static [u8] {
        return END_VOTING_PERIOD_CONTEXT
    }

    fn state(&self) -> Result<VotingSessionData<T::AccountId, BlockNumberFor<T>>, DispatchError> {
        if <Summary<T> as Store>::VotesRepository::contains_key(self.root_id) {
            return Ok(Summary::<T>::get_vote(self.root_id))
        }
        return Err(DispatchError::Other("Root Id is not found in votes repository"))
    }

    fn is_valid(&self) -> bool {
        let voting_session_data = self.state();
        let root_data_result = Summary::<T>::try_get_root_data(&self.root_id);
        let root_is_pending_approval =
            <Summary<T> as Store>::PendingApproval::contains_key(&self.root_id.range);
        let voting_session_exists_for_root =
            <Summary<T> as Store>::VotesRepository::contains_key(&self.root_id);

        if root_data_result.is_err() ||
            !root_is_pending_approval ||
            !voting_session_exists_for_root ||
            voting_session_data.is_err()
        {
            return false
        }

        let root_already_accepted =
            root_data_result.expect("already checked for error").is_validated;
        let pending_approval_root_ingress_counter =
            <Summary<T> as Store>::PendingApproval::get(self.root_id.range);
        let vote_is_for_correct_version_of_root_range =
            pending_approval_root_ingress_counter == self.root_id.ingress_counter;

        return !root_already_accepted && vote_is_for_correct_version_of_root_range
    }

    fn is_active(&self) -> bool {
        let voting_session_data = self.state();
        return voting_session_data.is_ok() &&
            <frame_system::Pallet<T>>::block_number() <
                voting_session_data.expect("voting session data is ok").end_of_voting_period &&
            self.is_valid()
    }

    fn record_approve_vote(&self, voter: T::AccountId) -> DispatchResult {
        <Summary<T> as Store>::VotesRepository::try_mutate(
            &self.root_id,
            |vote| -> DispatchResult {
                vote.ayes.try_push(voter).map_err(|_| avn_error::<T>::VectorBoundsExceeded)?;
                Ok(())
            },
        )?;
        Ok(())
    }

    fn record_reject_vote(&self, voter: T::AccountId) -> DispatchResult {
        <Summary<T> as Store>::VotesRepository::try_mutate(
            &self.root_id,
            |vote| -> DispatchResult {
                vote.nays.try_push(voter).map_err(|_| avn_error::<T>::VectorBoundsExceeded)?;
                Ok(())
            },
        )?;
        Ok(())
    }

    fn end_voting_session(&self, sender: T::AccountId) -> DispatchResult {
        return Summary::<T>::end_voting(sender, &self.root_id)
    }
}

/***************** Functions that run in an offchain worker context  **************** */

pub fn create_vote_lock_name<T: Config>(root_id: &RootId<BlockNumberFor<T>>) -> Vec<u8> {
    let mut name = b"vote_summary::hash::".to_vec();
    name.extend_from_slice(&mut root_id.range.from_block.encode());
    name.extend_from_slice(&mut root_id.range.to_block.encode());
    name.extend_from_slice(&mut root_id.ingress_counter.encode());
    name
}

fn is_vote_in_transaction_pool<T: Config>(root_id: &RootId<BlockNumberFor<T>>) -> bool {
    let persistent_data = create_vote_lock_name::<T>(root_id);
    return OcwLock::is_locked::<frame_system::Pallet<T>>(&persistent_data)
}

pub fn cast_votes_if_required<T: Config>(
    this_validator: &Validator<<T as avn::Config>::AuthorityId, T::AccountId>,
) {
    let root_ids: Vec<RootId<BlockNumberFor<T>>> = <Summary<T> as Store>::PendingApproval::iter()
        .filter(|(root_range, ingress_counter)| {
            let root_id = RootId::new(*root_range, *ingress_counter);
            root_can_be_voted_on::<T>(&root_id, &this_validator.account_id)
        })
        .take(MAX_VOTING_SESSIONS_RETURNED)
        .map(|(root_range, ingress_counter)| RootId::new(root_range, ingress_counter))
        .collect();

    // try to send 1 of MAX_VOTING_SESSIONS_RETURNED votes
    for root_id in root_ids {
        let vote_lock_name = create_vote_lock_name::<T>(&root_id);
        let mut lock = AVN::<T>::get_ocw_locker(&vote_lock_name);

        if let Ok(guard) = lock.try_lock() {
            let root_hash =
                Summary::<T>::compute_root_hash(root_id.range.from_block, root_id.range.to_block);

            if root_hash.is_err() {
                log::error!(
                    "💔️ Error getting root hash while signing root id {:?} to vote",
                    &root_id
                );
                continue
            }

            let root_data = Summary::<T>::try_get_root_data(&root_id);
            if let Err(e) = root_data {
                log::error!(
                    "💔️ Error getting root data while signing root id {:?} to vote. {:?}",
                    &root_id,
                    e
                );
                continue
            }

            if root_hash.expect("has valid hash") == root_data.expect("checked for error").root_hash
            {
                if send_approve_vote::<T>(&root_id, this_validator).is_err() {
                    // TODO: should we output any error message here?
                    continue
                }
            } else {
                if send_reject_vote::<T>(&root_id, this_validator).is_err() {
                    // TODO: should we output any error message here?
                    continue
                }
            }

            // keep the lock until it expires
            guard.forget();
            return
        } else {
            log::trace!(target: "avn", "🤷 Unable to acquire local lock for root {:?}. Lock probably exists already", &root_id);
            continue
        };
    }
}

pub fn end_voting_if_required<T: Config>(
    block_number: BlockNumberFor<T>,
    this_validator: &Validator<<T as avn::Config>::AuthorityId, T::AccountId>,
) {
    let root_ids: Vec<RootId<BlockNumberFor<T>>> = <Summary<T> as Store>::PendingApproval::iter()
        .filter(|(root_range, ingress_counter)| {
            let root_id = RootId::new(*root_range, *ingress_counter);
            block_number > Summary::<T>::get_vote(root_id).end_of_voting_period
        })
        .take(MAX_VOTING_SESSIONS_RETURNED)
        .map(|(root_range, ingress_counter)| RootId::new(root_range, ingress_counter))
        .collect();

    for root_id in root_ids {
        let voting_session_data = Summary::<T>::get_root_voting_session(&root_id).state();
        if voting_session_data.is_err() {
            log::error!(
                "💔 Error getting voting session data with root id {:?} to end voting period",
                &root_id
            );
            return
        }

        let voting_session_id =
            voting_session_data.expect("voting session data is ok").voting_session_id;
        let signature = match this_validator
            .key
            .sign(&(END_VOTING_PERIOD_CONTEXT, voting_session_id).encode())
        {
            Some(s) => s,
            _ => {
                log::error!("💔️ Error signing root id {:?} to end voting period", &root_id);
                return
            },
        };

        if let Err(e) = SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(
            Call::end_voting_period {
                root_id: root_id.clone(),
                validator: this_validator.clone(),
                signature,
            }
            .into(),
        ) {
            log::error!(
                "💔️ Error sending transaction to end vote for root id {:?}: {:?}",
                root_id,
                e
            );
        }
    }
}

fn root_can_be_voted_on<T: Config>(root_id: &RootId<BlockNumberFor<T>>, voter: &T::AccountId) -> bool {
    // There is an edge case here. If this is being run very close to `end_of_voting_period`, by the
    // time the vote gets mined. It may be outside the voting window and get rejected.
    let root_voting_session = Summary::<T>::get_root_voting_session(root_id);
    let voting_session_data = root_voting_session.state();
    return voting_session_data.is_ok() &&
        !voting_session_data.expect("voting session data is ok").has_voted(voter) &&
        !is_vote_in_transaction_pool::<T>(root_id) &&
        root_voting_session.is_active()
}

fn send_approve_vote<T: Config>(
    root_id: &RootId<BlockNumberFor<T>>,
    this_validator: &Validator<<T as avn::Config>::AuthorityId, T::AccountId>,
) -> Result<(), ()> {
    let approve_vote_extrinsic_signature =
        sign_for_approve_vote_extrinsic::<T>(root_id, this_validator)?;

    log::trace!(target: "avn", "🖊️  Worker sends approval vote for summary calculation: {:?}]", &root_id);

    if let Err(e) = SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(
        Call::approve_root {
            root_id: root_id.clone(),
            validator: this_validator.clone(),
            signature: approve_vote_extrinsic_signature,
        }
        .into(),
    ) {
        log::error!(
            "💔️ Error sending `approve vote transaction` for root id {:?}: {:?}",
            root_id,
            e
        );
        return Err(())
    }

    Ok(())
}

fn sign_for_approve_vote_extrinsic<T: Config>(
    root_id: &RootId<BlockNumberFor<T>>,
    this_validator: &Validator<<T as avn::Config>::AuthorityId, T::AccountId>,
) -> Result<<T::AuthorityId as RuntimeAppPublic>::Signature, ()> {
    let voting_session_data = Summary::<T>::get_root_voting_session(&root_id).state();
    if voting_session_data.is_err() {
        log::error!("💔 Error getting voting session data with root id {:?} to vote", &root_id);
        return Err(())
    }

    let voting_session_id =
        voting_session_data.expect("voting session data is ok").voting_session_id;
    let signature = this_validator
        .key
        .sign(&(CAST_VOTE_CONTEXT, voting_session_id, APPROVE_VOTE).encode());

    if signature.is_none() {
        log::error!("💔️ Error signing root id {:?} to vote", &root_id);
        return Err(())
    };

    return Ok(signature.expect("Signature is not empty if it gets here"))
}

fn send_reject_vote<T: Config>(
    root_id: &RootId<BlockNumberFor<T>>,
    this_validator: &Validator<<T as avn::Config>::AuthorityId, T::AccountId>,
) -> Result<(), ()> {
    let voting_session_data = Summary::<T>::get_root_voting_session(&root_id).state();
    if voting_session_data.is_err() {
        log::error!("💔 Error getting voting session data with root id {:?} to vote", &root_id);
        return Err(())
    }

    let voting_session_id =
        voting_session_data.expect("voting session data is ok").voting_session_id;
    let signature = this_validator
        .key
        .sign(&(CAST_VOTE_CONTEXT, voting_session_id, REJECT_VOTE).encode());

    if signature.is_none() {
        log::error!("💔️ Error signing root id {:?} to vote", &root_id);
        return Err(())
    };

    log::trace!(target: "avn", "🖊️  Worker sends reject vote for summary calculation: {:?}]", &root_id);

    if let Err(e) = SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(
        Call::reject_root {
            root_id: root_id.clone(),
            validator: this_validator.clone(),
            signature: signature.expect("We have a signature"),
        }
        .into(),
    ) {
        log::error!(
            "💔️ Error sending `reject vote transaction` for root id {:?}: {:?}",
            root_id,
            e
        );
        return Err(())
    }

    Ok(())
}
