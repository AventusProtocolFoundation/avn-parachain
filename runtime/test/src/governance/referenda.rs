// Copyright 2023 Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot. If not, see <http://www.gnu.org/licenses/>.

//! New governance configurations for the Polkadot runtime.

use super::*;
use frame_support::traits::EitherOf;
use runtime_common::constants::currency::*;
use sp_core::ConstU16;
use sp_runtime::traits::Replace;

parameter_types! {
    pub const VoteLockingPeriod: BlockNumber = 1 * DAYS;
}

impl pallet_conviction_voting::Config for Runtime {
    type WeightInfo = pallet_conviction_voting::weights::SubstrateWeight<Runtime>;
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type Polls = Referenda;
    type MaxTurnout = frame_support::traits::TotalIssuanceOf<Balances, Self::AccountId>;
    type MaxVotes = ConstU32<512>;
    type VoteLockingPeriod = VoteLockingPeriod;
}

parameter_types! {
    pub const AlarmInterval: BlockNumber = 1;
    pub const SubmissionDeposit: Balance = 50 * AVT;
    pub const UndecidingTimeout: BlockNumber = 14 * DAYS;
}

pub type GeneralAdminOrRoot = EitherOf<EnsureRoot<AccountId>, origins::GeneralAdmin>;

impl pallet_custom_origins::Config for Runtime {}

impl pallet_whitelist::Config for Runtime {
    type WeightInfo = pallet_whitelist::weights::SubstrateWeight<Runtime>;
    type RuntimeCall = RuntimeCall;
    type RuntimeEvent = RuntimeEvent;
    type WhitelistOrigin = WhitelistedCaller;
    type DispatchWhitelistedOrigin = EitherOf<EnsureRoot<Self::AccountId>, WhitelistedCaller>;
    type Preimages = Preimage;
}

// The purpose of this pallet is to queue calls to be dispatched as by root later => the Dispatch
// origin corresponds to the Gov2 Whitelist track.

pallet_referenda::impl_tracksinfo_get!(TracksInfo, Balance, BlockNumber);

impl pallet_referenda::Config for Runtime {
    type WeightInfo = pallet_referenda::weights::SubstrateWeight<Runtime>;
    type RuntimeCall = RuntimeCall;
    type RuntimeEvent = RuntimeEvent;
    type Scheduler = Scheduler;
    type Currency = Balances;
    type SubmitOrigin = frame_system::EnsureSigned<AccountId>;
    type CancelOrigin = EitherOf<EnsureRoot<Self::AccountId>, ReferendumCanceller>;
    type KillOrigin = EitherOf<EnsureRoot<Self::AccountId>, ReferendumKiller>;
    type Slash = ();
    type Votes = pallet_conviction_voting::VotesOf<Runtime>;
    type Tally = pallet_conviction_voting::TallyOf<Runtime>;
    type SubmissionDeposit = SubmissionDeposit;
    type MaxQueued = ConstU32<100>;
    type UndecidingTimeout = UndecidingTimeout;
    type AlarmInterval = AlarmInterval;
    type Tracks = TracksInfo;
    type Preimages = Preimage;
}
