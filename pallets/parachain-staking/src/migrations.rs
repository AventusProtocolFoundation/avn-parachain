// Copyright 2019-2022 PureStake Inc.
// This file is part of Moonbeam.

// Moonbeam is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Moonbeam is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Moonbeam.  If not, see <http://www.gnu.org/licenses/>.

//! # Migrations

#![allow(unused)]

use crate::nomination_requests::{NominationAction, ScheduledRequest};
use crate::pallet::{NominationScheduledRequests, NominatorState, Total};
#[allow(deprecated)]
use crate::types::deprecated::{NominationChange, Nominator as OldNominator};
use crate::types::Nominator;
use crate::{
	BalanceOf, Bond, BottomNominations, CandidateInfo, CandidateMetadata, CapacityStatus,
	CollatorCandidate, Config, Nominations, Event, Pallet, Points, Era, Staked, TopNominations,
};
#[cfg(feature = "try-runtime")]
use frame_support::traits::OnRuntimeUpgradeHelpersExt;
use frame_support::Twox64Concat;
extern crate alloc;
#[cfg(feature = "try-runtime")]
use alloc::format;
use frame_support::{
	migration::storage_key_iter,
	pallet_prelude::PhantomData,
	traits::{Get, OnRuntimeUpgrade, ReservableCurrency},
	weights::Weight,
};
#[cfg(feature = "try-runtime")]
use scale_info::prelude::string::String;
use sp_runtime::traits::{Saturating, Zero};
use sp_std::{convert::TryInto, vec::Vec};

/// Migration to move nominator requests towards a nomination, from [NominatorState] into
/// [NominationScheduledRequests] storage item.
/// Additionally [NominatorState] is migrated from [OldNominator] to [Nominator].
pub struct SplitNominatorStateIntoNominationScheduledRequests<T>(PhantomData<T>);

impl<T: Config> SplitNominatorStateIntoNominationScheduledRequests<T> {
	const PALLET_PREFIX: &'static [u8] = b"ParachainStaking";
	const NOMINATOR_STATE_PREFIX: &'static [u8] = b"NominatorState";

	#[allow(deprecated)]
	#[cfg(feature = "try-runtime")]
	fn old_request_to_string(
		nominator: &T::AccountId,
		request: &crate::deprecated::NominationRequest<T::AccountId, BalanceOf<T>>,
	) -> String {
		match request.action {
			NominationChange::Revoke => {
				format!(
					"nominator({:?})_when({})_Revoke({:?})",
					nominator, request.when_executable, request.amount
				)
			}
			NominationChange::Decrease => {
				format!(
					"nominator({:?})_when({})_Decrease({:?})",
					nominator, request.when_executable, request.amount
				)
			}
		}
	}

	#[cfg(feature = "try-runtime")]
	fn new_request_to_string(request: &ScheduledRequest<T::AccountId, BalanceOf<T>>) -> String {
		match request.action {
			NominationAction::Revoke(v) => {
				format!(
					"nominator({:?})_when({})_Revoke({:?})",
					request.nominator, request.when_executable, v
				)
			}
			NominationAction::Decrease(v) => {
				format!(
					"nominator({:?})_when({})_Decrease({:?})",
					request.nominator, request.when_executable, v
				)
			}
		}
	}
}

#[allow(deprecated)]
impl<T: Config> OnRuntimeUpgrade for SplitNominatorStateIntoNominationScheduledRequests<T> {
	fn on_runtime_upgrade() -> Weight {
		use sp_std::collections::btree_map::BTreeMap;

		log::info!(
			target: "SplitNominatorStateIntoNominationScheduledRequests",
			"running migration for NominatorState to new version and NominationScheduledRequests \
			storage item"
		);

		let mut reads: Weight = 0;
		let mut writes: Weight = 0;

		let mut scheduled_requests: BTreeMap<
			T::AccountId,
			Vec<ScheduledRequest<T::AccountId, BalanceOf<T>>>,
		> = BTreeMap::new();
		<NominatorState<T>>::translate(
			|nominator, old_state: OldNominator<T::AccountId, BalanceOf<T>>| {
				reads = reads.saturating_add(1);
				writes = writes.saturating_add(1);

				for (collator, request) in old_state.requests.requests.into_iter() {
					let action = match request.action {
						NominationChange::Revoke => NominationAction::Revoke(request.amount),
						NominationChange::Decrease => NominationAction::Decrease(request.amount),
					};
					let entry = scheduled_requests.entry(collator.clone()).or_default();
					entry.push(ScheduledRequest {
						nominator: nominator.clone(),
						when_executable: request.when_executable,
						action,
					});
				}

				let new_state = Nominator {
					id: old_state.id,
					nominations: old_state.nominations,
					total: old_state.total,
					less_total: old_state.requests.less_total,
					status: old_state.status,
				};

				Some(new_state)
			},
		);

		writes = writes.saturating_add(scheduled_requests.len() as Weight); // 1 write per request
		for (collator, requests) in scheduled_requests {
			<NominationScheduledRequests<T>>::insert(collator, requests);
		}

		T::DbWeight::get().reads_writes(reads, writes)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<(), &'static str> {
		let mut expected_nominator_state_entries = 0u64;
		let mut expected_requests = 0u64;
		for (_key, state) in migration::storage_iter::<OldNominator<T::AccountId, BalanceOf<T>>>(
			Self::PALLET_PREFIX,
			Self::NOMINATOR_STATE_PREFIX,
		) {
			Self::set_temp_storage(
				state.requests.less_total,
				&*format!("expected_nominator-{:?}_decrease_amount", state.id),
			);

			for (collator, request) in state.requests.requests.iter() {
				Self::set_temp_storage(
					Self::old_request_to_string(&state.id, &request),
					&*format!(
						"expected_collator-{:?}_nominator-{:?}_request",
						collator, state.id,
					),
				);
			}
			expected_nominator_state_entries = expected_nominator_state_entries.saturating_add(1);
			expected_requests =
				expected_requests.saturating_add(state.requests.requests.len() as u64);
		}

		Self::set_temp_storage(
			expected_nominator_state_entries,
			"expected_nominator_state_entries",
		);
		Self::set_temp_storage(expected_requests, "expected_requests");

		use frame_support::migration;

		Ok(())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade() -> Result<(), &'static str> {
		// Scheduled decrease amount (bond_less) is correctly migrated
		let mut actual_nominator_state_entries = 0;
		for (nominator, state) in <NominatorState<T>>::iter() {
			let expected_nominator_decrease_amount: BalanceOf<T> = Self::get_temp_storage(
				&*format!("expected_nominator-{:?}_decrease_amount", nominator),
			)
			.expect("must exist");
			assert_eq!(
				expected_nominator_decrease_amount, state.less_total,
				"decrease amount did not match for nominator {:?}",
				nominator,
			);
			actual_nominator_state_entries = actual_nominator_state_entries.saturating_add(1);
		}

		// Existing nominator state entries are not removed
		let expected_nominator_state_entries: u64 =
			Self::get_temp_storage("expected_nominator_state_entries").expect("must exist");
		assert_eq!(
			expected_nominator_state_entries, actual_nominator_state_entries,
			"unexpected change in the number of NominatorState entries"
		);

		// Scheduled requests are correctly migrated
		let mut actual_requests = 0u64;
		for (collator, scheduled_requests) in <NominationScheduledRequests<T>>::iter() {
			for request in scheduled_requests {
				let expected_nominator_request: String = Self::get_temp_storage(&*format!(
					"expected_collator-{:?}_nominator-{:?}_request",
					collator, request.nominator,
				))
				.expect("must exist");
				let actual_nominator_request = Self::new_request_to_string(&request);
				assert_eq!(
					expected_nominator_request, actual_nominator_request,
					"scheduled request did not match for collator {:?}, nominator {:?}",
					collator, request.nominator,
				);

				actual_requests = actual_requests.saturating_add(1);
			}
		}

		let expected_requests: u64 =
			Self::get_temp_storage("expected_requests").expect("must exist");
		assert_eq!(
			expected_requests, actual_requests,
			"number of scheduled request entries did not match",
		);

		Ok(())
	}
}

/// Migration to patch the incorrect nominations sums for all candidates
pub struct PatchIncorrectNominationSums<T>(PhantomData<T>);
impl<T: Config> OnRuntimeUpgrade for PatchIncorrectNominationSums<T> {
	fn on_runtime_upgrade() -> Weight {
		log::info!(
			target: "PatchIncorrectNominationSums",
			"running migration to patch incorrect nomination sums"
		);
		let pallet_prefix: &[u8] = b"ParachainStaking";
		let top_nominations_prefix: &[u8] = b"TopNominations";
		let bottom_nominations_prefix: &[u8] = b"BottomNominations";
		// Read all the data into memory.
		// https://crates.parity.io/frame_support/storage/migration/fn.storage_key_iter.html
		let stored_top_nominations: Vec<_> = storage_key_iter::<
			T::AccountId,
			Nominations<T::AccountId, BalanceOf<T>>,
			Twox64Concat,
		>(pallet_prefix, top_nominations_prefix)
		.collect();
		let migrated_candidates_top_count: Weight = stored_top_nominations
			.len()
			.try_into()
			.expect("There are between 0 and 2**64 mappings stored.");
		let stored_bottom_nominations: Vec<_> = storage_key_iter::<
			T::AccountId,
			Nominations<T::AccountId, BalanceOf<T>>,
			Twox64Concat,
		>(pallet_prefix, bottom_nominations_prefix)
		.collect();
		let migrated_candidates_bottom_count: Weight = stored_bottom_nominations
			.len()
			.try_into()
			.expect("There are between 0 and 2**64 mappings stored.");
		fn fix_nominations<T: Config>(
			nominations: Nominations<T::AccountId, BalanceOf<T>>,
		) -> Nominations<T::AccountId, BalanceOf<T>> {
			let correct_total = nominations
				.nominations
				.iter()
				.fold(BalanceOf::<T>::zero(), |acc, b| acc + b.amount);
			log::info!(
				target: "PatchIncorrectNominationSums",
				"Correcting total from {:?} to {:?}",
				nominations.total, correct_total
			);
			Nominations {
				nominations: nominations.nominations,
				total: correct_total,
			}
		}
		for (account, old_top_nominations) in stored_top_nominations {
			let new_top_nominations = fix_nominations::<T>(old_top_nominations);
			let mut candidate_info = <CandidateInfo<T>>::get(&account)
				.expect("TopNominations exists => CandidateInfo exists");
			candidate_info.total_counted = candidate_info.bond + new_top_nominations.total;
			if candidate_info.is_active() {
				Pallet::<T>::update_active(account.clone(), candidate_info.total_counted);
			}
			<CandidateInfo<T>>::insert(&account, candidate_info);
			<TopNominations<T>>::insert(&account, new_top_nominations);
		}
		for (account, old_bottom_nominations) in stored_bottom_nominations {
			let new_bottom_nominations = fix_nominations::<T>(old_bottom_nominations);
			<BottomNominations<T>>::insert(&account, new_bottom_nominations);
		}
		let weight = T::DbWeight::get();
		let top = migrated_candidates_top_count.saturating_mul(3 * weight.write + 3 * weight.read);
		let bottom = migrated_candidates_bottom_count.saturating_mul(weight.write + weight.read);
		// 20% max block weight as margin for error
		top + bottom + 100_000_000_000
	}
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<(), &'static str> {
		// get total counted for all candidates
		for (account, state) in <CandidateInfo<T>>::iter() {
			Self::set_temp_storage(
				state.total_counted,
				&format!("Candidate{:?}TotalCounted", account)[..],
			);
		}
		Ok(())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade() -> Result<(), &'static str> {
		// ensure new total counted = top_nominations.sum() + collator self bond
		for (account, state) in <CandidateInfo<T>>::iter() {
			let old_count =
				Self::get_temp_storage(&format!("Candidate{:?}TotalCounted", account)[..])
					.expect("qed");
			let new_count = state.total_counted;
			let top_nominations_sum = <TopNominations<T>>::get(account)
				.expect("CandidateInfo exists => TopNominations exists")
				.nominations
				.iter()
				.fold(BalanceOf::<T>::zero(), |acc, b| acc + b.amount);
			let correct_total_counted = top_nominations_sum + state.bond;
			assert_eq!(new_count, correct_total_counted);
			if new_count != old_count {
				log::info!(
					target: "PatchIncorrectNominationSums",
					"Corrected total from {:?} to {:?}",
					old_count, new_count
				);
			}
		}
		Ok(())
	}
}

/*
/// Migration to split CandidateState and minimize unnecessary storage reads
/// for PoV optimization
/// This assumes Config::MaxTopNominationsPerCandidate == OldConfig::MaxNominatorsPerCandidate
pub struct SplitCandidateStateToDecreasePoV<T>(PhantomData<T>);
impl<T: Config> OnRuntimeUpgrade for SplitCandidateStateToDecreasePoV<T> {
	fn on_runtime_upgrade() -> Weight {
		let pallet_prefix: &[u8] = b"ParachainStaking";
		let storage_item_prefix: &[u8] = b"CandidateState";
		// Read all the data into memory.
		// https://crates.parity.io/frame_support/storage/migration/fn.storage_key_iter.html
		let stored_data: Vec<_> = storage_key_iter::<
			T::AccountId,
			CollatorCandidate<T::AccountId, BalanceOf<T>>,
			Twox64Concat,
		>(pallet_prefix, storage_item_prefix)
		.collect();
		let migrated_count: Weight = stored_data
			.len()
			.try_into()
			.expect("There are between 0 and 2**64 mappings stored.");
		// Now remove the old storage
		// https://crates.parity.io/frame_support/storage/migration/fn.remove_storage_prefix.html
		remove_storage_prefix(pallet_prefix, storage_item_prefix, &[]);
		// Assert that old storage is empty
		assert!(storage_key_iter::<
			T::AccountId,
			CollatorCandidate<T::AccountId, BalanceOf<T>>,
			Twox64Concat,
		>(pallet_prefix, storage_item_prefix)
		.next()
		.is_none());
		for (account, state) in stored_data {
			// all nominations are stored greatest to least post migration
			// but bottom nominations were least to greatest pre migration
			let new_bottom_nominations: Vec<Bond<T::AccountId, BalanceOf<T>>> =
				if state.bottom_nominations.len()
					> T::MaxBottomNominationsPerCandidate::get() as usize
				{
					// if actual length > max bottom nominations, revoke the bottom actual - max
					let rest = state.bottom_nominations.len()
						- T::MaxBottomNominationsPerCandidate::get() as usize;
					let mut total_less = BalanceOf::<T>::zero();
					state.bottom_nominations.iter().take(rest).for_each(
						|Bond { owner, amount }| {
							total_less = total_less.saturating_add(*amount);
							// update nominator state
							// unreserve kicked bottom
							T::Currency::unreserve(&owner, *amount);
							let mut nominator_state = <NominatorState<T>>::get(&owner)
								.expect("Nomination existence => NominatorState existence");
							let leaving = nominator_state.nominations.0.len() == 1usize;
							nominator_state.rm_nomination::<T>(&account);
							Pallet::<T>::deposit_event(Event::NominationKicked {
								nominator: owner.clone(),
								candidate: account.clone(),
								unstaked_amount: *amount,
							});
							if leaving {
								<NominatorState<T>>::remove(&owner);
								Pallet::<T>::deposit_event(Event::NominatorLeft {
									nominator: owner.clone(),
									unstaked_amount: *amount,
								});
							} else {
								<NominatorState<T>>::insert(&owner, nominator_state);
							}
						},
					);
					let new_total = <Total<T>>::get() - total_less;
					<Total<T>>::put(new_total);
					state
						.bottom_nominations
						.into_iter()
						.rev()
						.take(T::MaxBottomNominationsPerCandidate::get() as usize)
						.collect()
				} else {
					state.bottom_nominations.into_iter().rev().collect()
				};
			let lowest_top_nomination_amount = if state.top_nominations.is_empty() {
				BalanceOf::<T>::zero()
			} else {
				state.top_nominations[state.top_nominations.len() - 1].amount
			};
			let highest_bottom_nomination_amount = if new_bottom_nominations.is_empty() {
				BalanceOf::<T>::zero()
			} else {
				new_bottom_nominations[0].amount
			};
			// start here,
			let lowest_bottom_nomination_amount = if new_bottom_nominations.is_empty() {
				BalanceOf::<T>::zero()
			} else {
				new_bottom_nominations[new_bottom_nominations.len() - 1].amount
			};
			let top_capacity = match &state.top_nominations {
				x if x.len() as u32 >= T::MaxTopNominationsPerCandidate::get() => {
					CapacityStatus::Full
				}
				x if x.is_empty() => CapacityStatus::Empty,
				_ => CapacityStatus::Partial,
			};
			let bottom_capacity = match &new_bottom_nominations {
				x if x.len() as u32 >= T::MaxBottomNominationsPerCandidate::get() => {
					CapacityStatus::Full
				}
				x if x.is_empty() => CapacityStatus::Empty,
				_ => CapacityStatus::Partial,
			};
			let metadata = CandidateMetadata {
				bond: state.bond,
				nomination_count: state.top_nominations.len() as u32
					+ new_bottom_nominations.len() as u32,
				total_counted: state.total_counted,
				lowest_top_nomination_amount,
				highest_bottom_nomination_amount,
				lowest_bottom_nomination_amount,
				top_capacity,
				bottom_capacity,
				request: state.request,
				status: state.state,
			};
			<CandidateInfo<T>>::insert(&account, metadata);
			let top_nominations = Nominations {
				total: state.total_counted - state.bond,
				nominations: state.top_nominations,
			};
			<TopNominations<T>>::insert(&account, top_nominations);
			let bottom_nominations = Nominations {
				total: new_bottom_nominations
					.iter()
					.fold(BalanceOf::<T>::zero(), |acc, b| acc + b.amount),
				nominations: new_bottom_nominations,
			};
			<BottomNominations<T>>::insert(&account, bottom_nominations);
		}
		let weight = T::DbWeight::get();
		migrated_count.saturating_mul(3 * weight.write + weight.read)
	}
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<(), &'static str> {
		// get nomination count for all candidates to check consistency
		for (account, state) in <CandidateState<T>>::iter() {
			// insert top + bottom into some temp map?
			let total_nomination_count =
				state.top_nominations.len() as u32 + state.bottom_nominations.len() as u32;
			Self::set_temp_storage(
				total_nomination_count,
				&format!("Candidate{:?}NominationCount", account)[..],
			);
		}
		Ok(())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade() -> Result<(), &'static str> {
		// check that top + bottom are the same as the expected (stored in temp)
		for (account, state) in <CandidateInfo<T>>::iter() {
			let expected_count: u32 =
				Self::get_temp_storage(&format!("Candidate{:?}NominationCount", account)[..])
					.expect("qed");
			let actual_count = state.nomination_count;
			assert_eq!(expected_count, actual_count);
		}
		Ok(())
	}
}
*/

/// Migration to replace the automatic ExitQueue with a manual exits API.
/// This migration is idempotent so it can be run more than once without any risk.
// pub struct RemoveExitQueue<T>(PhantomData<T>);
// impl<T: Config> OnRuntimeUpgrade for RemoveExitQueue<T> {
// 	fn on_runtime_upgrade() -> Weight {
// 		log::info!(target: "RemoveExitQueue", "running migration to remove staking exit queue");
// 		let exit_queue = <ExitQueue2<T>>::take();
// 		let (mut reads, mut writes) = (1u64, 0u64);
// 		let mut nominator_exits: BTreeMap<T::AccountId, EraIndex> = BTreeMap::new();
// 		let mut nomination_revocations: BTreeMap<T::AccountId, (T::AccountId, EraIndex)> =
// 			BTreeMap::new();
// 		// Track scheduled nominator exits and revocations before migrating state
// 		// Candidates already track exit info locally so no tracking is necessary
// 		for (nominator, is_revocation, when) in exit_queue.nominator_schedule {
// 			if let Some(revoking_candidate) = is_revocation {
// 				nomination_revocations.insert(nominator, (revoking_candidate, when));
// 			} else {
// 				nominator_exits.insert(nominator, when);
// 			}
// 		}
// 		// execute candidate migration
// 		for (candidate_id, collator_state) in <CollatorState2<T>>::drain() {
// 			let candidate_state: CollatorCandidate<T::AccountId, BalanceOf<T>> =
// 				collator_state.into();
// 			<CandidateState<T>>::insert(candidate_id, candidate_state);
// 			reads += 1u64;
// 			writes += 1u64;
// 		}
// 		// execute nominator migration
// 		for (nominator_id, nominator_state) in <NominatorState2<T>>::drain() {
// 			let mut nominator_state =
// 				migrate_nominator_to_nominator_state::<T>(nominator_id.clone(), nominator_state);
// 			// add exit if it exists
// 			if let Some(when) = nominator_exits.get(&nominator_id) {
// 				nominator_state.set_leaving(*when);
// 			}
// 			// add revocation if exists
// 			if let Some((candidate, when)) = nomination_revocations.get(&nominator_id) {
// 				nominator_state.hotfix_set_revoke::<T>(candidate.clone(), *when);
// 			}
// 			<NominatorState<T>>::insert(nominator_id, nominator_state);
// 			reads += 1u64;
// 			writes += 1u64;
// 		}
// 		let db_weight = T::DbWeight::get();
// 		if reads > 1u64 {
// 			// 50% of the max block weight as safety margin for computation
// 			db_weight.reads(reads) + db_weight.writes(writes) + 250_000_000_000
// 		} else {
// 			// migration was already executed before
// 			db_weight.reads(reads)
// 		}
// 	}

// 	#[cfg(feature = "try-runtime")]
// 	fn pre_upgrade() -> Result<(), &'static str> {
// 		use frame_support::storage::migration::storage_iter;

// 		let pallet_prefix: &[u8] = b"ParachainStaking";
// 		let collator_state_prefix: &[u8] = b"CollatorState2";
// 		let nominator_state_prefix: &[u8] = b"NominatorState2";

// 		// Assert new storage is empty
// 		assert!(CandidateState::<T>::iter().next().is_none());
// 		assert!(NominatorState::<T>::iter().next().is_none());

// 		// Check number of old collator candidates, and set it aside in temp storage
// 		let old_collator_count = storage_iter::<Collator2<T::AccountId, BalanceOf<T>>>(
// 			pallet_prefix,
// 			collator_state_prefix,
// 		)
// 		.count() as u64;
// 		Self::set_temp_storage(old_collator_count, "old_collator_count");

// 		// Read first old candidate from old storage and set it aside in temp storage
// 		if old_collator_count > 0 {
// 			let example_collator = storage_key_iter::<
// 				T::AccountId,
// 				Collator2<T::AccountId, BalanceOf<T>>,
// 				Twox64Concat,
// 			>(pallet_prefix, collator_state_prefix)
// 			.next()
// 			.expect("We already confirmed that there was at least one item stored");

// 			Self::set_temp_storage(example_collator, "example_collator");
// 		}

// 		// Check number of new nominators, and set it aside in temp storage
// 		let old_nominator_count = storage_iter::<Nominator2<T::AccountId, BalanceOf<T>>>(
// 			pallet_prefix,
// 			nominator_state_prefix,
// 		)
// 		.count() as u64;
// 		Self::set_temp_storage(old_nominator_count, "old_nominator_count");

// 		// Read first new nominator from old storage and set it aside in temp storage
// 		if old_nominator_count > 0 {
// 			let example_nominator = storage_key_iter::<
// 				T::AccountId,
// 				Nominator2<T::AccountId, BalanceOf<T>>,
// 				Twox64Concat,
// 			>(pallet_prefix, nominator_state_prefix)
// 			.next()
// 			.expect("We already confirmed that there was at least one item stored");

// 			Self::set_temp_storage(example_nominator, "example_nominator");
// 		}
// 		Ok(())
// 	}

// 	#[cfg(feature = "try-runtime")]
// 	fn post_upgrade() -> Result<(), &'static str> {
// 		// Check number of candidates matches what was set aside in pre_upgrade
// 		let old_candidate_count: u64 = Self::get_temp_storage("old_collator_count")
// 			.expect("We stored the old collator candidate count so it should be there");
// 		let new_candidate_count = CandidateState::<T>::iter().count() as u64;
// 		assert_eq!(old_candidate_count, new_candidate_count);

// 		// Check that our example candidate is converted correctly
// 		if new_candidate_count > 0 {
// 			let (account, original_collator_state): (
// 				T::AccountId,
// 				Collator2<T::AccountId, BalanceOf<T>>,
// 			) = Self::get_temp_storage("example_collator").expect("qed");
// 			let new_candidate_state = CandidateState::<T>::get(account).expect("qed");
// 			let old_candidate_converted: CollatorCandidate<T::AccountId, BalanceOf<T>> =
// 				original_collator_state.into();
// 			assert_eq!(new_candidate_state, old_candidate_converted);
// 		}

// 		// Check number of nominators matches what was set aside in pre_upgrade
// 		let old_nominator_count: u64 = Self::get_temp_storage("old_nominator_count")
// 			.expect("We stored the old nominator count so it should be there");
// 		let new_nominator_count = NominatorState::<T>::iter().count() as u64;
// 		assert_eq!(old_nominator_count, new_nominator_count);

// 		// Check that our example nominator is converted correctly
// 		if new_nominator_count > 0 {
// 			let (account, original_nominator_state): (
// 				T::AccountId,
// 				Nominator2<T::AccountId, BalanceOf<T>>,
// 			) = Self::get_temp_storage("example_nominator").expect("qed");
// 			let new_nominator_state = NominatorState::<T>::get(&account).expect("qed");
// 			let old_nominator_converted: Nominator<T::AccountId, BalanceOf<T>> =
// 				migrate_nominator_to_nominator_state::<T>(account, original_nominator_state);
// 			assert_eq!(old_nominator_converted, new_nominator_state);
// 		}
// 		Ok(())
// 	}
// }

/// Migration to purge staking storage bloat for `Points` and `AtStake` storage items
pub struct PurgeStaleStorage<T>(PhantomData<T>);
impl<T: Config> OnRuntimeUpgrade for PurgeStaleStorage<T> {
	fn on_runtime_upgrade() -> Weight {
		log::info!(target: "PurgeStaleStorage", "running migration to remove storage bloat");
		let current_era = <Era<T>>::get().current;
		let payment_delay = T::RewardPaymentDelay::get();
		let db_weight = T::DbWeight::get();
		let (reads, mut writes) = (3u64, 0u64);
		if current_era <= payment_delay {
			// early enough so no storage bloat exists yet
			// (only relevant for chains <= payment_delay eras old)
			return db_weight.reads(reads);
		}
		// already paid out at the beginning of current era
		let most_recent_era_to_kill = current_era - payment_delay;
		for i in 1..=most_recent_era_to_kill {
			writes = writes.saturating_add(2u64);
			<Staked<T>>::remove(i);
			<Points<T>>::remove(i);
		}
		// 5% of the max block weight as safety margin for computation
		db_weight.reads(reads) + db_weight.writes(writes) + 25_000_000_000
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<(), &'static str> {
		// trivial migration
		Ok(())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade() -> Result<(), &'static str> {
		// expect only the storage items for the last 2 eras to be stored
		let staked_count = Staked::<T>::iter().count() as u32;
		let points_count = Points::<T>::iter().count() as u32;
		let delay = T::RewardPaymentDelay::get();
		assert_eq!(
			staked_count, delay,
			"Expected {} for `Staked` count, Found: {}",
			delay, staked_count
		);
		assert_eq!(
			points_count, delay,
			"Expected {} for `Points` count, Found: {}",
			delay, staked_count
		);
		Ok(())
	}
}
