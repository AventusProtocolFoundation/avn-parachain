#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::string::String;

use codec::{Decode, Encode, MaxEncodedLen};
use sp_avn_common::{
    event_types::Validator,
    offchain_worker_storage_lock::{self as OcwLock},
};
use sp_std::{marker::PhantomData, prelude::*};

use frame_support::{
    dispatch::{DispatchError, DispatchResult},
    log,
};
use frame_system::offchain::SubmitTransaction;
use pallet_avn::{self as avn, vote::*, Error as avn_error};
use sp_application_crypto::RuntimeAppPublic;
use sp_core::ecdsa;
use sp_runtime::{scale_info::TypeInfo, traits::Zero};
use sp_std::fmt::Debug;

use super::{Call, Config};
use crate::{BalanceOf, GrowthId, Pallet as ParachainStaking, Store, AVN};

pub const CAST_VOTE_CONTEXT: &'static [u8] = b"growth_casting_vote";
pub const END_VOTING_PERIOD_CONTEXT: &'static [u8] = b"growth_end_voting_period";
const MAX_VOTING_SESSIONS_RETURNED: usize = 5;

#[derive(PartialEq, Eq, Clone, Encode, Decode, Default, Debug, MaxEncodedLen, TypeInfo)]
pub struct GrowthVotingSession<T: Config> {
    growth_id: GrowthId,
    phantom: PhantomData<T>,
}

impl<T: Config> GrowthVotingSession<T> {
    pub fn new(growth_id: &GrowthId) -> Self {
        return GrowthVotingSession { growth_id: growth_id.clone(), phantom: Default::default() }
    }
}

impl<T: Config> VotingSessionManager<T::AccountId, T::BlockNumber> for GrowthVotingSession<T> {
    fn cast_vote_context(&self) -> &'static [u8] {
        return CAST_VOTE_CONTEXT
    }

    fn end_voting_period_context(&self) -> &'static [u8] {
        return END_VOTING_PERIOD_CONTEXT
    }

    fn state(&self) -> Result<VotingSessionData<T::AccountId, T::BlockNumber>, DispatchError> {
        if <ParachainStaking<T> as Store>::VotesRepository::contains_key(self.growth_id) {
            return Ok(ParachainStaking::<T>::get_vote(self.growth_id))
        }
        return Err(DispatchError::Other("Growth data is not found in votes repository"))
    }

    fn is_valid(&self) -> bool {
        let voting_session_data = self.state();
        let growth_info_result = ParachainStaking::<T>::try_get_growth_data(&self.growth_id.period);
        let growth_is_pending_approval =
            <ParachainStaking<T> as Store>::PendingApproval::contains_key(&self.growth_id.period);
        let voting_session_exists_for_growth =
            <ParachainStaking<T> as Store>::VotesRepository::contains_key(&self.growth_id);

        if growth_info_result.is_err() ||
            !growth_is_pending_approval ||
            !voting_session_exists_for_growth ||
            voting_session_data.is_err()
        {
            return false
        }

        let pending_approval_growth_ingress_counter =
            <ParachainStaking<T> as Store>::PendingApproval::get(self.growth_id.period);
        let vote_is_for_correct_ingress_counter =
            pending_approval_growth_ingress_counter == self.growth_id.ingress_counter;

        let voting_session_is_finalised =
            AVN::<T>::is_block_finalised(voting_session_data.expect("checked").created_at_block);

        return vote_is_for_correct_ingress_counter && voting_session_is_finalised
    }

    fn is_active(&self) -> bool {
        let voting_session_data = self.state();
        return voting_session_data.is_ok() &&
            <frame_system::Pallet<T>>::block_number() <
                voting_session_data.expect("voting session data is ok").end_of_voting_period &&
            self.is_valid()
    }

    fn record_approve_vote(
        &self,
        voter: T::AccountId,
        approval_signature: ecdsa::Signature,
    ) -> DispatchResult {
        <ParachainStaking<T> as Store>::VotesRepository::try_mutate(
            &self.growth_id,
            |vote| -> DispatchResult {
                vote.ayes.try_push(voter).map_err(|_| avn_error::<T>::VectorBoundsExceeded)?;
                vote.confirmations
                    .try_push(approval_signature)
                    .map_err(|_| avn_error::<T>::VectorBoundsExceeded)?;
                Ok(())
            },
        )?;
        Ok(())
    }

    fn record_reject_vote(&self, voter: T::AccountId) -> DispatchResult {
        <ParachainStaking<T> as Store>::VotesRepository::try_mutate(
            &self.growth_id,
            |vote| -> DispatchResult {
                vote.nays.try_push(voter).map_err(|_| avn_error::<T>::VectorBoundsExceeded)?;
                Ok(())
            },
        )?;
        Ok(())
    }

    fn end_voting_session(&self, sender: T::AccountId) -> DispatchResult {
        return ParachainStaking::<T>::end_voting(sender, &self.growth_id)
    }
}

/***************** Functions that run in an offchain worker context  **************** */

fn growth_is_valid<T: Config>(growth_id: &GrowthId) -> bool {
    let growth_info_result = ParachainStaking::<T>::try_get_growth_data(&growth_id.period);

    if growth_info_result.is_err() {
        return false
    }

    let growth_info = growth_info_result.expect("checked for error");
    let growth_values_are_valid = growth_info.total_staker_reward > BalanceOf::<T>::zero() &&
        growth_info.total_stake_accumulated > BalanceOf::<T>::zero();
    let growth_already_processed =
        <ParachainStaking<T> as Store>::ProcessedGrowthPeriods::contains_key(growth_id.period) ||
            growth_info.triggered == Some(true);
    let growth_period_is_complete =
        growth_id.period < <ParachainStaking<T> as Store>::GrowthPeriod::get().index;

    return !growth_already_processed && growth_period_is_complete && growth_values_are_valid
}

pub fn create_vote_lock_name<T: Config>(growth_id: &GrowthId) -> OcwLock::PersistentId {
    let mut name = b"vote_growth::hash::".to_vec();
    name.extend_from_slice(&mut growth_id.period.encode());
    name.extend_from_slice(&mut growth_id.ingress_counter.encode());
    name
}

fn is_vote_in_transaction_pool<T: Config>(period: &GrowthId) -> bool {
    let persistent_data = create_vote_lock_name::<T>(period);
    return OcwLock::is_locked(&persistent_data)
}

pub fn cast_votes_if_required<T: Config>(
    block_number: T::BlockNumber,
    this_validator: &Validator<<T as avn::Config>::AuthorityId, T::AccountId>,
) {
    let growth_ids: Vec<GrowthId> = <ParachainStaking<T> as Store>::PendingApproval::iter()
        .filter(|(period, ingress_counter)| {
            let growth_id = GrowthId::new(*period, *ingress_counter);
            growth_can_be_voted_on::<T>(&growth_id, &this_validator.account_id)
        })
        .take(MAX_VOTING_SESSIONS_RETURNED)
        .map(|(period, ingress_counter)| GrowthId::new(period, ingress_counter))
        .collect();

    // try to send 1 of MAX_VOTING_SESSIONS_RETURNED votes
    for growth_id in growth_ids {
        if OcwLock::set_lock_with_expiry(
            block_number,
            ParachainStaking::<T>::lock_till_request_expires(),
            create_vote_lock_name::<T>(&growth_id),
        )
        .is_err()
        {
            log::trace!(target: "avn", "🤷 Unable to acquire local lock for growth {:?}. Lock probably exists already", &growth_id);
            continue
        }

        if growth_is_valid::<T>(&growth_id) {
            if send_approve_vote::<T>(&growth_id, this_validator).is_err() {
                // TODO: should we output any error message here?
                continue
            }
        } else {
            if send_reject_vote::<T>(&growth_id, this_validator).is_err() {
                // TODO: should we output any error message here?
                continue
            }
        }
    }
}

pub fn end_voting_if_required<T: Config>(
    block_number: T::BlockNumber,
    this_validator: &Validator<<T as avn::Config>::AuthorityId, T::AccountId>,
) {
    let growth_ids: Vec<GrowthId> = <ParachainStaking<T> as Store>::PendingApproval::iter()
        .filter(|(period, ingress_counter)| {
            block_number >
                ParachainStaking::<T>::get_vote(GrowthId::new(*period, *ingress_counter))
                    .end_of_voting_period
        })
        .take(MAX_VOTING_SESSIONS_RETURNED)
        .map(|(period, ingress_counter)| GrowthId::new(period, ingress_counter))
        .collect();

    for growth_id in growth_ids {
        let voting_session_data =
            ParachainStaking::<T>::get_growth_voting_session(&growth_id).state();
        if voting_session_data.is_err() {
            log::error!(
                "💔 Error getting voting session data with growth id {:?} to end voting period",
                &growth_id
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
                log::error!("💔️ Error signing growth id {:?} to end voting period", &growth_id);
                return
            },
        };

        if let Err(e) = SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(
            Call::end_voting_period {
                growth_id: growth_id.clone(),
                validator: this_validator.clone(),
                signature,
            }
            .into(),
        ) {
            log::error!(
                "💔️ Error sending transaction to end vote for growth id {:?}: {:?}",
                growth_id,
                e
            );
        }
    }
}

fn growth_can_be_voted_on<T: Config>(growth_id: &GrowthId, voter: &T::AccountId) -> bool {
    // There is an edge case here. If this is being run very close to `end_of_voting_period`, by the
    // time the vote gets mined. It may be outside the voting window and get rejected.
    let growth_voting_session = ParachainStaking::<T>::get_growth_voting_session(growth_id);
    let voting_session_data = growth_voting_session.state();
    return voting_session_data.is_ok() &&
        !voting_session_data.expect("voting session data is ok").has_voted(voter) &&
        !is_vote_in_transaction_pool::<T>(growth_id) &&
        growth_voting_session.is_active()
}

fn send_approve_vote<T: Config>(
    growth_id: &GrowthId,
    this_validator: &Validator<<T as avn::Config>::AuthorityId, T::AccountId>,
) -> Result<(), ()> {
    let (eth_encoded_data, eth_signature) =
        ParachainStaking::<T>::sign_growth_for_ethereum(&growth_id).map_err(|_| ())?;

    let approve_vote_extrinsic_signature = sign_for_approve_vote_extrinsic::<T>(
        growth_id,
        this_validator,
        eth_encoded_data,
        &eth_signature,
    )?;

    log::trace!(target: "avn", "🖊️  Worker sends approval vote for triggering growth: {:?}]", &growth_id);

    if let Err(e) = SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(
        Call::approve_growth {
            growth_id: growth_id.clone(),
            validator: this_validator.clone(),
            approval_signature: eth_signature,
            signature: approve_vote_extrinsic_signature,
        }
        .into(),
    ) {
        log::error!(
            "💔️ Error sending `approve vote transaction` for growth id {:?}: {:?}",
            growth_id,
            e
        );
        return Err(())
    }

    Ok(())
}

fn sign_for_approve_vote_extrinsic<T: Config>(
    growth_id: &GrowthId,
    this_validator: &Validator<<T as avn::Config>::AuthorityId, T::AccountId>,
    eth_encoded_data: String,
    eth_signature: &ecdsa::Signature,
) -> Result<<T::AuthorityId as RuntimeAppPublic>::Signature, ()> {
    let voting_session_data = ParachainStaking::<T>::get_growth_voting_session(&growth_id).state();
    if voting_session_data.is_err() {
        log::error!("💔 Error getting voting session data with growth id {:?} to vote", &growth_id);
        return Err(())
    }

    let voting_session_id =
        voting_session_data.expect("voting session data is ok").voting_session_id;
    let signature = this_validator.key.sign(
        &(
            CAST_VOTE_CONTEXT,
            voting_session_id,
            APPROVE_VOTE,
            eth_encoded_data.encode(),
            eth_signature.encode(),
        )
            .encode(),
    );

    if signature.is_none() {
        log::error!("💔️ Error signing growth id {:?} to vote", &growth_id);
        return Err(())
    };

    return Ok(signature.expect("Signature is not empty if it gets here"))
}

fn send_reject_vote<T: Config>(
    growth_id: &GrowthId,
    this_validator: &Validator<<T as avn::Config>::AuthorityId, T::AccountId>,
) -> Result<(), ()> {
    let voting_session_data = ParachainStaking::<T>::get_growth_voting_session(&growth_id).state();
    if voting_session_data.is_err() {
        log::error!("💔 Error getting voting session data with growth id {:?} to vote", &growth_id);
        return Err(())
    }

    let voting_session_id =
        voting_session_data.expect("voting session data is ok").voting_session_id;
    let signature = this_validator
        .key
        .sign(&(CAST_VOTE_CONTEXT, voting_session_id, REJECT_VOTE).encode());

    if signature.is_none() {
        log::error!("💔️ Error signing growth id {:?} to vote", &growth_id);
        return Err(())
    };

    log::trace!(target: "avn", "🖊️  Worker sends reject vote for triggering growth: {:?}]", &growth_id);

    if let Err(e) = SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(
        Call::reject_growth {
            growth_id: growth_id.clone(),
            validator: this_validator.clone(),
            signature: signature.expect("We have a signature"),
        }
        .into(),
    ) {
        log::error!(
            "💔️ Error sending `reject vote transaction` for growth id {:?}: {:?}",
            growth_id,
            e
        );
        return Err(())
    }

    Ok(())
}
