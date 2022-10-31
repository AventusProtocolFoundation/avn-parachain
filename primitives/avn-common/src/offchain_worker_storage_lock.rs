// Copyright 2020 Artos Systems (UK) Ltd.

use codec::{Codec, Decode, Encode};
#[cfg(feature = "std")]
use log::trace;
use sp_runtime::traits::Member;
use sp_runtime::{
    offchain::storage::{MutateStorageError, StorageRetrievalError, StorageValueRef},
    traits::AtLeast32Bit,
};
use sp_std::{if_std, vec, vec::Vec};

const MODULE_ID: &'static [u8] = b"ocw::storagelock";
const UNEXPECTED_STATE: () = ();

pub enum OcwStorageError {
    OffchainWorkerAlreadyRun,
    ErrorRecordingOffchainWorkerRun,
}

#[derive(Clone, Copy)]
pub enum OcwOperationExpiration {
    Fast,
    Slow,
    Custom(u32),
}

impl OcwOperationExpiration {
    pub fn block_delay(&self) -> u32 {
        match self {
            OcwOperationExpiration::Fast => 10,
            OcwOperationExpiration::Slow => 100,
            OcwOperationExpiration::Custom(count) => *count,
        }
    }
}

pub type PersistentId = Vec<u8>;
type LockData = u64;

#[derive(Default, Clone, Debug, PartialEq)]
struct LocalDBEntry<BlockNumber: Member + Codec, Data: Encode> {
    pub expiry: BlockNumber,
    pub data: Data,
    pub persistent_id: PersistentId,
}

impl<BlockNumber: Member + Codec + AtLeast32Bit, Data: Encode> LocalDBEntry<BlockNumber, Data> {
    pub fn new(
        current_block: BlockNumber,
        expiry_type: OcwOperationExpiration,
        data: Data,
        persistent_id: PersistentId,
    ) -> Self {
        LocalDBEntry::<BlockNumber, Data> {
            expiry: current_block + BlockNumber::from(expiry_type.block_delay()),
            data,
            persistent_id,
        }
    }
}

fn generate_name_for_block_expiring_list<BlockNumber: Member + Encode>(
    block_number: &BlockNumber,
) -> PersistentId {
    let mut name = b"expiring_at_block:".to_vec();
    name.extend_from_slice(&mut block_number.encode());
    return name;
}

fn generate_name_for_last_run_block(caller_id: Vec<u8>) -> PersistentId {
    let name = [caller_id.as_slice(), b"::last_run"].concat();
    return name;
}

// TODO [TYPE: business logic][PRI: medium] make this function reject entries with expiry in already expired blocks
fn insert_item_to_expiry_list<BlockNumber: Member + Codec>(
    new_db_entry: &LocalDBEntry<BlockNumber, LockData>,
) -> Option<()> {
    const ALREADY_INSERTED: () = ();
    let storage_name_of_block_item_list =
        generate_name_for_block_expiring_list(&new_db_entry.expiry);
    let items_to_expire_on_block = StorageValueRef::persistent(&storage_name_of_block_item_list);

    let registration_result = items_to_expire_on_block.mutate(
        |data: Result<Option<Vec<PersistentId>>, StorageRetrievalError>| match data {
            Err(_) => Err(UNEXPECTED_STATE),
            Ok(Some(mut expiration_list)) => {
                if !expiration_list.contains(&new_db_entry.persistent_id) {
                    expiration_list.push(new_db_entry.persistent_id.clone());
                    return Ok(expiration_list);
                } else {
                    return Err(ALREADY_INSERTED);
                }
            },
            _ => Ok(vec![new_db_entry.persistent_id.clone()]),
        },
    );
    match registration_result {
        Ok(_) => Some(()),
        _ => {
            if_std! {
                trace!(
                    target: "avn",
                    "🤷 Unable to add [{}] to the expiry list, already exists.",
                    sp_std::str::from_utf8(&new_db_entry.persistent_id).unwrap_or("-")
                );
            }
            None
        },
    }
}

fn get_expiring_list_for_block<BlockNumber: Member + Codec>(
    block_number: &BlockNumber,
) -> Option<Vec<PersistentId>> {
    let expiring_list_name = generate_name_for_block_expiring_list(block_number);
    let expiring_list_storage = StorageValueRef::persistent(&expiring_list_name);
    if let Ok(Some(stored_data)) = expiring_list_storage.get::<Vec<PersistentId>>() {
        if stored_data.len() != 0 {
            return Some(stored_data);
        }
    }
    None
}

fn remove_entry_from_local_db(entry: &PersistentId) {
    let mut expired = StorageValueRef::persistent(entry);
    expired.clear();
}

fn read_data_from_local_db<Data: Decode>(persistent_id: &PersistentId) -> Option<Data> {
    let entry = StorageValueRef::persistent(persistent_id);
    if let Ok(Some(stored_data)) = entry.get::<Data>() {
        return Some(stored_data);
    }
    None
}

fn remove_lock_entries_from_block<BlockNumber: Member + Codec + AtLeast32Bit>(
    block_number: &BlockNumber,
    to_remove: &PersistentId,
) -> Result<(), ()> {
    remove_entry_from_local_db(&to_remove);
    // Now remove from the expiration list
    let storage_name_of_block_item_list = generate_name_for_block_expiring_list(block_number);
    let items_to_expire_on_block = StorageValueRef::persistent(&storage_name_of_block_item_list);

    const ENTRY_NOT_PRESENT: () = ();

    let removal_result = items_to_expire_on_block.mutate(
        |data: Result<Option<Vec<PersistentId>>, StorageRetrievalError>| match data {
            Ok(None) => Err(UNEXPECTED_STATE),
            Ok(Some(mut expiration_list)) => {
                let find_index = expiration_list.iter().position(|r| r[..] == to_remove[..]);
                if let Some(index) = find_index {
                    expiration_list.remove(index);
                    return Ok(expiration_list);
                } else {
                    return Err(ENTRY_NOT_PRESENT);
                }
            },
            _ => Err(ENTRY_NOT_PRESENT),
        },
    );
    match removal_result {
        Ok(_) => Ok(()),
        _ => Err(()),
    }
}

/****************************** Public functions ***********************************/

pub fn set_lock_with_expiry<BlockNumber: Member + Codec + AtLeast32Bit>(
    current_block: BlockNumber,
    expiry_type: OcwOperationExpiration,
    persistent_id: PersistentId,
) -> Result<(), ()> {
    const DUPLICATE_ENTRY: () = ();

    let new_db_entry = LocalDBEntry::new(current_block, expiry_type, 1 as LockData, persistent_id);

    if insert_item_to_expiry_list(&new_db_entry).is_none() {
        return Err(());
    }

    let entry = StorageValueRef::persistent(&new_db_entry.persistent_id);
    let registration_result =
        entry.mutate(|data: Result<Option<LockData>, StorageRetrievalError>| match data {
            Ok(Some(_existing_entry)) => Err(DUPLICATE_ENTRY),
            _ => Ok(new_db_entry.data.clone()),
        });

    match registration_result {
        Ok(_) => Ok(()),
        _ => {
            if_std! {
                trace!(
                    target: "avn",
                    "🤷 Unable to acquire local lock for [{}]. Lock exists already",
                    sp_std::str::from_utf8(&new_db_entry.persistent_id).unwrap_or("-")
                );
            }
            Err(())
        },
    }
}

pub fn is_locked(persistent_id: &PersistentId) -> bool {
    let entry = StorageValueRef::persistent(persistent_id);
    if let Ok(Some(_)) = entry.get::<LockData>() {
        return true;
    }
    return false;
}

pub fn cleanup_expired_entries<BlockNumber: Member + Codec + Copy + AtLeast32Bit>(
    block_number: &BlockNumber,
) {
    let mut cleanup_range = vec![*block_number];

    let last_cleanup_block = read_data_from_local_db::<BlockNumber>(
        &generate_name_for_last_run_block(MODULE_ID.to_vec()),
    )
    .unwrap_or(BlockNumber::from(1 as u32));

    // This would be much easier using core::ops::Range between the block numbers and the collect or iterate the values.
    // core::iter::Step must be implemented for BlockNumber in order to Iterate or collect from it.
    // Unfortunately this functionality for generics is only available on nightly builds and is experimental.
    let mut block_to_clean = *block_number;
    while block_to_clean > last_cleanup_block {
        cleanup_range.push(block_to_clean);
        block_to_clean = block_to_clean - BlockNumber::from(1 as u32);
    }

    for block in cleanup_range {
        if let Some(expired_items) = get_expiring_list_for_block(&block) {
            for expired_entry_id in expired_items.iter() {
                remove_entry_from_local_db(expired_entry_id);
            }
            let expiring_list_name = generate_name_for_block_expiring_list(&block);
            remove_entry_from_local_db(&expiring_list_name);
        }
    }
    let _ = record_block_run::<BlockNumber>(*block_number, MODULE_ID.to_vec());
}

pub fn record_block_run<BlockNumber: Member + Codec + AtLeast32Bit>(
    block_number: BlockNumber,
    caller_id: Vec<u8>,
) -> Result<(), OcwStorageError> {
    const ALREADY_RUN: () = ();
    let key = generate_name_for_last_run_block(caller_id);
    let val = StorageValueRef::persistent(&key);
    // Using `mutate` means that only one worker will be able to "acquire a lock" to update this value.
    let result = val.mutate(|last_run: Result<Option<BlockNumber>, StorageRetrievalError>| {
        match last_run {
            // If we already have a value in storage and the value is the same or greater than the current block_number
            // we abort the update as a worker from a newer block has beaten us here.
            Ok(Some(block)) if block >= block_number => Err(ALREADY_RUN),
            // In every other case we attempt to acquire the lock and update the block_number.
            _ => Ok(block_number),
        }
    });

    match result {
        Ok(_) => Ok(()),
        Err(MutateStorageError::ValueFunctionFailed(ALREADY_RUN)) => {
            Err(OcwStorageError::OffchainWorkerAlreadyRun)
        },
        //We didn't get a lock to update the value so return false
        Err(MutateStorageError::ConcurrentModification(_)) => {
            Err(OcwStorageError::ErrorRecordingOffchainWorkerRun)
        },
    }
}

pub fn remove_storage_lock<BlockNumber: Member + Codec + AtLeast32Bit>(
    creation_block: BlockNumber,
    expiry_type: OcwOperationExpiration,
    persistent_id: PersistentId,
) -> Result<(), ()> {
    let db_entry_to_remove =
        LocalDBEntry::new(creation_block, expiry_type, 1 as LockData, persistent_id);
    remove_lock_entries_from_block(&db_entry_to_remove.expiry, &db_entry_to_remove.persistent_id)
}

// ======================================== Tests =====================================================

#[cfg(test)]
#[path = "tests/test_offchain_worker_storage_locks.rs"]
mod test_offchain_worker_storage_locks;
