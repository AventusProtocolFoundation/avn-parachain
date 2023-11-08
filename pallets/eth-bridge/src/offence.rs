use codec::{Decode, Encode, MaxEncodedLen};
use pallet_session::{historical::IdentificationTuple, Config as SessionConfig};
use scale_info::TypeInfo;
use sp_runtime::Perbill;
use sp_staking::{
    offence::{Kind, Offence},
    SessionIndex,
};

#[derive(PartialEq, Eq, Clone, Debug, Encode, Decode, TypeInfo)]
pub enum EthBridgeCorroborationOffenceType {
    ChallengeAttemptedOnValidResult,
}

#[derive(PartialEq, Clone, Debug, Encode, Decode, TypeInfo)]
pub struct EthBridgeCorroborationOffence<Offender> {
    pub session_index: SessionIndex,
    pub offenders: Vec<Offender>,
    pub offence_type: EthBridgeCorroborationOffenceType,
    pub validator_set_count: u32,
}

impl<Offender: Clone> Offence<Offender> for EthBridgeCorroborationOffence<Offender> {
    const ID: Kind = *b"ethbridge:offence";
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

    fn slash_fraction(&self, offenders_count: u32) -> Perbill {
        Perbill::from_percent(100)
    }
}

pub fn create_offenders_identification<T: crate::Config>(
    offenders_accounts: &Vec<T::AccountId>,
) -> Vec<IdentificationTuple<T>> {
    let offenders = offenders_accounts
        .into_iter()
        .filter_map(|id| <T as SessionConfig>::ValidatorIdOf::convert(id.clone()))
        .filter_map(|id| T::FullIdentificationOf::convert(id.clone()).map(|full_id| (id, full_id)))
        .collect::<Vec<IdentificationTuple<T>>>();
    return offenders
}
