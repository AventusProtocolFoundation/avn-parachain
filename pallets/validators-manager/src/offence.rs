//Copyright 2022 Aventus Network Services (UK) Ltd.

use sp_runtime::Perbill;
use sp_staking::{
    offence::{Kind, Offence},
    SessionIndex,
};

use codec::{Decode, Encode};
use frame_support::log;
use pallet_session::{historical::IdentificationTuple, Config as SessionConfig};
use sp_runtime::{scale_info::TypeInfo, traits::Convert};
use sp_staking::offence::ReportOffence;
use sp_std::prelude::*;

#[derive(PartialEq, Eq, Clone, Debug, Encode, Decode, TypeInfo)]
pub enum ValidatorOffenceType {
    InvalidSignatureSubmitted,
    RejectedValidAction,
    ApprovedInvalidAction,
}
use crate::Event;

#[derive(PartialEq, Clone, Debug, Encode, Decode)]
pub struct ValidatorOffence<Offender> {
    /// The current session index in which we report the validators that submitted an invalid
    /// ethereum log.
    pub session_index: SessionIndex,
    /// The size of the validator set in current session/era.
    pub validator_set_count: u32,
    /// Authorities that validated the invalid log.
    pub offenders: Vec<Offender>,
    /// The different types of the offence
    pub offence_type: ValidatorOffenceType,
}

impl<Offender: Clone> Offence<Offender> for ValidatorOffence<Offender> {
    const ID: Kind = *b"validator:offenc";
    type TimeSlot = SessionIndex;

    fn offenders(&self) -> Vec<Offender> {
        self.offenders.clone()
    }

    fn session_index(&self) -> SessionIndex {
        self.session_index
    }

    fn validator_set_count(&self) -> u32 {
        self.validator_set_count
    }

    fn time_slot(&self) -> Self::TimeSlot {
        self.session_index
    }

    fn slash_fraction(&self, _offenders: u32) -> Perbill {
        // We don't implement fraction slashes at the moment.
        Perbill::from_percent(100)
    }
}

fn create_offenders_identification<T: crate::Config>(
    offenders_accounts: &Vec<T::AccountId>,
) -> Vec<IdentificationTuple<T>> {
    let offenders = offenders_accounts
        .into_iter()
        .filter_map(|id| <T as SessionConfig>::ValidatorIdOf::convert(id.clone()))
        .filter_map(|id| T::FullIdentificationOf::convert(id.clone()).map(|full_id| (id, full_id)))
        .collect::<Vec<IdentificationTuple<T>>>();
    return offenders
}

pub fn create_and_report_validators_offence<T: crate::Config>(
    reporter: &T::AccountId,
    offenders_accounts: &Vec<T::AccountId>,
    offence_type: ValidatorOffenceType,
) {
    let offenders = create_offenders_identification::<T>(offenders_accounts);

    if offenders.len() > 0 {
        let invalid_event_offence = ValidatorOffence {
            session_index: <pallet_session::Pallet<T>>::current_index(),
            validator_set_count: <pallet_session::Pallet<T>>::validators().len() as u32,
            offenders: offenders.clone(),
            offence_type: offence_type.clone(),
        };

        if !T::ReportValidatorOffence::is_known_offence(
            &invalid_event_offence.offenders(),
            &invalid_event_offence.time_slot(),
        ) {
            let reporters = vec![reporter.clone()];
            if let Err(e) =
                T::ReportValidatorOffence::report_offence(reporters, invalid_event_offence)
            {
                log::info!(
                    target: "pallet-validators-manager",
                    "ℹ️ Error while reporting offence: {:?}. Stored in deferred",
                    e
                );
            }
            <crate::Pallet<T>>::deposit_event(Event::<T>::OffenceReported {
                offence_type,
                offenders,
            });
        }
    }
}
