#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::string::String;

use codec::{Decode, Encode};
use sp_avn_common::{
    event_types::Validator,
    ocw_lock::{self as OcwLock},
};
use sp_std::prelude::*;

use frame_support::{
    dispatch::{DispatchError, DispatchResult},
    log,
};
use frame_system::offchain::SubmitTransaction;
use pallet_avn::{self as avn, vote::*, AccountToBytesConverter, Error as avn_error};
use sp_application_crypto::RuntimeAppPublic;
use sp_std::fmt::Debug;

use crate::{
    ActionId, Call, Config, IngressCounter, Pallet as ValidatorsManager, Store,
    ValidatorsActionStatus, AVN,
};
use pallet_ethereum_transactions::ethereum_transaction::EthTransactionType;

pub const CAST_VOTE_CONTEXT: &'static [u8] = b"validators_manager_casting_vote";
pub const END_VOTING_PERIOD_CONTEXT: &'static [u8] = b"validators_manager_end_voting_period";
const MAX_VOTING_SESSIONS_RETURNED: usize = 5;

#[derive(PartialEq, Eq, Clone, Encode, Decode, Default, Debug)]
pub struct ValidatorManagementVotingSession<T: Config> {
    pub action_id: ActionId<T::AccountId>,
}

impl<T: Config> ValidatorManagementVotingSession<T> {
    pub fn new(action_id: &ActionId<T::AccountId>) -> Self {
        return ValidatorManagementVotingSession::<T> { action_id: action_id.clone() }
    }
}

impl<T: Config> VotingSessionManager<T::AccountId, T::BlockNumber>
    for ValidatorManagementVotingSession<T>
{
    fn cast_vote_context(&self) -> &'static [u8] {
        return CAST_VOTE_CONTEXT
    }

    fn end_voting_period_context(&self) -> &'static [u8] {
        return END_VOTING_PERIOD_CONTEXT
    }

    fn state(&self) -> Result<VotingSessionData<T::AccountId, T::BlockNumber>, DispatchError> {
        if <ValidatorsManager<T> as Store>::VotesRepository::contains_key(&self.action_id) {
            return Ok(ValidatorsManager::<T>::get_vote(self.action_id.clone()))
        }
        return Err(DispatchError::Other(
            "Action for this Validator Id is not found in votes repository",
        ))
    }

    fn is_valid(&self) -> bool {
        let voting_session_data = self.state();
        return voting_session_data.is_ok() &&
            <ValidatorsManager<T> as Store>::PendingApprovals::contains_key(
                &self.action_id.action_account_id,
            ) &&
            <ValidatorsManager<T> as Store>::VotesRepository::contains_key(&self.action_id)
    }

    fn is_active(&self) -> bool {
        let voting_session_data = self.state();
        return voting_session_data.is_ok() &&
            self.is_valid() &&
            <frame_system::Pallet<T>>::block_number() <
                voting_session_data.expect("voting session data is ok").end_of_voting_period
    }

    // TODO [TYPE: business logic][PRI: high][JIRA: 299][CRITICAL]: Store the approval signatures.
    // As per SYS-299's current proposal, validators can give an Eth Signature that proves they
    // have validated and approved this deregistration request
    fn record_approve_vote(&self, voter: T::AccountId) -> DispatchResult {
        if is_not_own_activation::<T>(&voter, self.action_id.ingress_counter) {
            <ValidatorsManager<T> as Store>::VotesRepository::try_mutate(
                &self.action_id,
                |vote| -> DispatchResult {
                    vote.ayes.try_push(voter).map_err(|_| avn_error::<T>::VectorBoundsExceeded)?;

                    Ok(())
                },
            )?;
        }
        Ok(())
    }

    fn record_reject_vote(&self, voter: T::AccountId) -> DispatchResult {
        <ValidatorsManager<T> as Store>::VotesRepository::try_mutate(
            &self.action_id,
            |vote| -> DispatchResult {
                vote.nays.try_push(voter).map_err(|_| avn_error::<T>::VectorBoundsExceeded)?;
                Ok(())
            },
        )?;
        Ok(())
    }

    fn end_voting_session(&self, sender: T::AccountId) -> DispatchResult {
        return ValidatorsManager::<T>::end_voting(sender, &self.action_id)
    }
}

/* ************ Functions that run in an offchain worker context ************ */

pub fn create_vote_lock_name<T: Config>(action_id: &ActionId<T::AccountId>) -> Vec<u8> {
    let mut name = b"vote_val_man::hash::".to_vec();
    name.extend_from_slice(&mut action_id.action_account_id.encode());
    name.extend_from_slice(&mut action_id.ingress_counter.encode());
    name
}

fn is_vote_in_transaction_pool<T: Config>(action_id: &ActionId<T::AccountId>) -> bool {
    let persistent_data = create_vote_lock_name::<T>(action_id);
    return OcwLock::is_locked::<frame_system::Pallet<T>>(&persistent_data)
}

// TODO this will not filter cases where another validator that is not activated, submits a
// signature
fn is_not_own_activation<T: Config>(
    account_id: &T::AccountId,
    ingress_counter: IngressCounter,
) -> bool {
    if let Some(action_data) =
        <ValidatorsManager<T> as Store>::ValidatorActions::get(account_id, ingress_counter)
    {
        if let EthTransactionType::ActivateCollator(activation_data) =
            action_data.reserved_eth_transaction
        {
            return activation_data.t2_public_key !=
                <T as Config>::AccountToBytesConvert::into_bytes(&account_id)
        }
        // If None, treat as it isn't an own activation.
        return true
    }
    return true
}

pub fn cast_votes_if_required<T: Config>(
    this_validator: &Validator<<T as avn::Config>::AuthorityId, T::AccountId>,
) {
    let pending_actions_ids: Vec<ActionId<T::AccountId>> =
        <ValidatorsManager<T> as Store>::PendingApprovals::iter()
            .filter(|(action_validator, ingress_counter)| {
                let action_id = ActionId::new(action_validator.clone(), *ingress_counter);
                action_can_be_voted_on::<T>(&action_id, &this_validator.account_id) &&
                    is_not_own_activation::<T>(&this_validator.account_id, *ingress_counter)
            })
            .take(MAX_VOTING_SESSIONS_RETURNED)
            .map(|(action_validator_id, ingress_counter)| {
                ActionId::new(action_validator_id, ingress_counter)
            })
            .collect();
    log::debug!(
        "📨 Actions to vote upon {:?}, from {:?}",
        pending_actions_ids.len(),
        <ValidatorsManager<T> as Store>::PendingApprovals::iter().count()
    );

    // try to send 1 of MAX_VOTING_SESSIONS_RETURNED votes
    for action_id in pending_actions_ids {
        let vote_lock_name = create_vote_lock_name::<T>(&action_id);
        let mut lock = AVN::<T>::get_ocw_locker(&vote_lock_name);

        if let Ok(guard) = lock.try_lock() {
            let validators_action_data_result =
                ValidatorsManager::<T>::try_get_validators_action_data(&action_id);
            if validators_action_data_result.is_err() {
                continue
            }

            if validators_action_data_result.expect("action data is valid").status ==
                ValidatorsActionStatus::Confirmed
            {
                if let Err(_) = send_approve_vote::<T>(&action_id, this_validator) {
                    continue
                }
            } else {
                if let Err(_) = send_reject_vote::<T>(&action_id, this_validator) {
                    continue
                }
            }

            // keep the lock until it expires
            guard.forget();
            return
        } else {
            log::trace!(target: "avn", "🤷 Unable to acquire local lock for action_id {:?}", &action_id);
            continue
        };
    }
}

pub fn end_voting_if_required<T: Config>(
    block_number: T::BlockNumber,
    this_validator: &Validator<<T as avn::Config>::AuthorityId, T::AccountId>,
) {
    let pending_actions_ids: Vec<ActionId<T::AccountId>> =
        <ValidatorsManager<T> as Store>::PendingApprovals::iter()
            .filter(|(deregistered_validator, ingress_counter)| {
                let action_id = ActionId::new(deregistered_validator.clone(), *ingress_counter);
                block_number > ValidatorsManager::<T>::get_vote(action_id).end_of_voting_period
            })
            .take(MAX_VOTING_SESSIONS_RETURNED)
            .map(|(action_account_id, ingress_counter)| {
                ActionId::new(action_account_id, ingress_counter)
            })
            .collect();

    // TODO [TYPE: security][PRI: high][CRITICAL][JIRA: 152]: consider adding `block_number` to the
    // signature to prevent signature re-use.
    for action_id in pending_actions_ids {
        let signature = match this_validator
            .key
            .sign(&(END_VOTING_PERIOD_CONTEXT, &action_id.encode()).encode())
        {
            Some(s) => s,
            _ => {
                log::error!("💔 Error signing action id {:?} to end voting period", action_id);
                return
            },
        };

        if let Err(e) = SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(
            Call::end_voting_period {
                action_id: action_id.clone(),
                validator: this_validator.clone(),
                signature,
            }
            .into(),
        ) {
            log::error!(
                "💔 Error sending transaction to end vote for action id {:?}: {:?}",
                action_id,
                e
            );
        }
    }
}

fn action_can_be_voted_on<T: Config>(
    action_id: &ActionId<T::AccountId>,
    voter: &T::AccountId,
) -> bool {
    // There is an edge case here. If this is being run very close to `end_of_voting_period`, by the
    // time the vote gets mined. It may be outside the voting window and get rejected.
    let voting_session = ValidatorsManager::<T>::get_voting_session(action_id);
    let voting_session_data = voting_session.state();
    log::debug!(
        "📨 voting_session data: {:#?} - voting_session_data are OK: {:?} \
        - voting session active: {:?} - voting session is valid: {:?} - vote already in pool: {:?}",
        voting_session_data,
        voting_session_data.is_ok(),
        voting_session.is_active(),
        voting_session.is_valid(),
        is_vote_in_transaction_pool::<T>(action_id)
    );

    return voting_session_data.is_ok() &&
        !voting_session_data.expect("voting session data is ok").has_voted(voter) &&
        voting_session.is_active() &&
        !is_vote_in_transaction_pool::<T>(action_id)
}

fn send_approve_vote<T: Config>(
    action_id: &ActionId<T::AccountId>,
    this_validator: &Validator<<T as avn::Config>::AuthorityId, T::AccountId>,
) -> Result<(), ()> {
    let approve_vote_extrinsic_signature =
        sign_for_approve_vote_extrinsic::<T>(action_id, this_validator)?;

    if let Err(e) = SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(
        Call::approve_validator_action {
            action_id: action_id.clone(),
            validator: this_validator.clone(),
            signature: approve_vote_extrinsic_signature,
        }
        .into(),
    ) {
        log::error!(
            "💔 Error sending `approve vote transaction` for action id {:?}: {:?}",
            action_id,
            e
        );

        return Err(())
    }

    Ok(())
}

fn sign_for_approve_vote_extrinsic<T: Config>(
    action_id: &ActionId<T::AccountId>,
    this_validator: &Validator<<T as avn::Config>::AuthorityId, T::AccountId>,
) -> Result<<T::AuthorityId as RuntimeAppPublic>::Signature, ()> {
    let signature = this_validator
        .key
        .sign(&(CAST_VOTE_CONTEXT, &action_id.encode(), APPROVE_VOTE).encode());

    if signature.is_none() {
        log::error!("💔 Error signing action id {:?} to vote", &action_id);
        return Err(())
    };

    return Ok(signature.expect("Signature is not empty if it gets here"))
}

fn send_reject_vote<T: Config>(
    action_id: &ActionId<T::AccountId>,
    this_validator: &Validator<<T as avn::Config>::AuthorityId, T::AccountId>,
) -> Result<(), ()> {
    let signature = this_validator
        .key
        .sign(&(CAST_VOTE_CONTEXT, &action_id.encode(), REJECT_VOTE).encode());
    if signature.is_none() {
        log::error!("💔 Error signing action id {:?} to vote", action_id);
        return Err(())
    };

    if let Err(e) = SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(
        Call::reject_validator_action {
            action_id: action_id.clone(),
            validator: this_validator.clone(),
            signature: signature.expect("We have a signature"),
        }
        .into(),
    ) {
        log::error!(
            "💔 Error sending `reject vote transaction` for action id {:?}: {:?}",
            action_id,
            e
        );

        return Err(())
    }

    Ok(())
}
