extern crate alloc;

use alloc::vec::Vec;

use core::marker::PhantomData;
use frame_support::pallet_prelude::*;
use scale_info::TypeInfo;
use sp_core::H256;
use sp_runtime::{traits::AtLeast32Bit, Perbill, RuntimeDebug};

pub type ProposalId = H256;

#[derive(Encode, Decode, RuntimeDebug, Clone, PartialEq, Eq, TypeInfo)]
pub enum RawPayload {
    /// Small proposals that can fit safely in the runtime
    Inline(Vec<u8>),

    /// A link to off-chain proposal data (e.g. IPFS hash)
    Uri(Vec<u8>),
}

#[derive(Encode, Decode, RuntimeDebug, Clone, PartialEq, Eq, TypeInfo, MaxEncodedLen)]
pub enum DecisionRule {
    /// Yes > No to win
    SimpleMajority,
    /// Yes > No AND turnout >= min_turnout (percent of snapshot).
    MajorityWithTurnout { min_turnout: Perbill },
    /// Yes / (Yes+No) >= threshold AND turnout >= min_turnout (optional).
    Threshold { threshold: Perbill },
}

#[derive(Encode, Decode, RuntimeDebug, Clone, PartialEq, Eq, TypeInfo, MaxEncodedLen)]
pub enum ProposalSource {
    /// External proposals created by other users. These require manual review and voting.
    External,
    /// Proposals created by other pallets. These can be voted on automatically by the pallet.
    Internal(ProposalType),
}

#[derive(Encode, Decode, RuntimeDebug, Clone, PartialEq, Eq, TypeInfo, MaxEncodedLen)]
pub enum ProposalType {
    Summary,
    Anchor,
    Governance,
    Other(u8),
}

#[derive(Encode, Decode, RuntimeDebug, Clone, PartialEq, Eq, TypeInfo, MaxEncodedLen)]
pub enum ProposalStatusEnum {
    Queued,
    Ongoing,
    Resolved { passed: bool },
    Cancelled,
    Unknown,
}

//implement default for ProposalStatusEnum to be Unknown
impl Default for ProposalStatusEnum {
    fn default() -> Self {
        ProposalStatusEnum::Unknown
    }
}

#[derive(Encode, Decode, RuntimeDebug, Clone, PartialEq, Eq, TypeInfo)]
pub struct ProposalRequest {
    pub title: Vec<u8>,
    pub payload: RawPayload,
    pub rule: DecisionRule,
    pub source: ProposalSource,
    /// A unique ref provided by the proposer. Used when sending notifications about this proposal.
    pub external_ref: H256,
    pub created_at: u32,
    pub vote_duration: Option<u32>,
}

// Interface for other pallets to interact with the watchtower pallet
pub trait WatchtowerInterface {
    type AccountId: Parameter;

    fn submit_proposal(
        proposer: Option<Self::AccountId>,
        proposal: ProposalRequest,
    ) -> DispatchResult;

    fn get_voting_status(proposal_id: ProposalId) -> ProposalStatusEnum;
    fn get_proposer(proposal_id: ProposalId) -> Option<Self::AccountId>;
}

// A simple no-op implementation of the WatchtowerInterface trait
pub struct NoopWatchtower<AccountId>(PhantomData<AccountId>);
impl<AccountId> WatchtowerInterface for NoopWatchtower<AccountId>
where
    AccountId: Parameter + Member + MaxEncodedLen + TypeInfo + Eq + core::fmt::Debug,
{
    type AccountId = AccountId;

    fn submit_proposal(_a: Option<Self::AccountId>, _p: ProposalRequest) -> DispatchResult {
        Ok(())
    }

    fn get_voting_status(_id: ProposalId) -> ProposalStatusEnum {
        ProposalStatusEnum::Unknown
    }

    fn get_proposer(_id: ProposalId) -> Option<Self::AccountId> {
        None
    }
}

pub trait WatchtowerHooks<P: Parameter> {
    /// Called when Watchtower raises an alert/notification.
    fn on_proposal_submitted(proposal_id: ProposalId, proposal: P) -> DispatchResult;
    fn on_consensus_reached(
        proposal_id: ProposalId,
        external_ref: &H256,
        approved: bool,
    ) -> DispatchResult;
    fn on_cancelled(proposal_id: ProposalId, external_ref: &H256) -> DispatchResult;
}

#[impl_trait_for_tuples::impl_for_tuples(30)]
impl<P: Parameter> WatchtowerHooks<P> for Tuple {
    fn on_proposal_submitted(proposal_id: ProposalId, proposal: P) -> DispatchResult {
        for_tuples!( #( Tuple::on_proposal_submitted(proposal_id, proposal.clone())?; )* );
        Ok(())
    }

    fn on_consensus_reached(
        proposal_id: ProposalId,
        external_ref: &H256,
        approved: bool,
    ) -> DispatchResult {
        for_tuples!( #( Tuple::on_consensus_reached(proposal_id, external_ref, approved)?; )* );
        Ok(())
    }

    fn on_cancelled(proposal_id: ProposalId, external_ref: &H256) -> DispatchResult {
        for_tuples!( #( Tuple::on_cancelled(proposal_id, external_ref)?; )* );
        Ok(())
    }
}
