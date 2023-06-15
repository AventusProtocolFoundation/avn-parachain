use frame_support::log;
use pallet_ethereum_transactions::ethereum_transaction::{
    ActivateCollatorData, DeregisterCollatorData,
};
use sp_io::hashing::keccak_256;

const PACKED_KEYS_SIZE: usize = 96;

#[cfg(test)]
#[path = "tests/confirmation_tests.rs"]
mod confirmation_tests;

/// This function generates the compacted call data needed to generate a confirmation for
/// registering a new collator. The implementation must match this schema:
/// https://github.com/Aventus-Network-Services/avn-bridge/blob/v1.1.0/contracts/AVNBridge.sol#L344-L345

pub(crate) fn concat_and_hash_activation_data(
    activate_collator_data: &ActivateCollatorData,
) -> [u8; 32] {
    let mut activate_collator_params_concat: [u8; PACKED_KEYS_SIZE] = [0u8; PACKED_KEYS_SIZE];

    activate_collator_params_concat[0..64]
        .copy_from_slice(&activate_collator_data.t1_public_key.as_bytes()[0..64]);
    activate_collator_params_concat[64..PACKED_KEYS_SIZE]
        .copy_from_slice(&activate_collator_data.t2_public_key[0..32]);

    let activate_collator_hash = keccak_256(&activate_collator_params_concat);

    log::info!(
        "🗜️ Creating packed hash for {:?} transaction: Concat params data (hex encoded): {:?} - keccak_256 hash (hex encoded): {:?}",
            &activate_collator_data,
            hex::encode(activate_collator_params_concat),
            hex::encode(activate_collator_hash)
    );
    return activate_collator_hash
}

/// This function generates the compacted call data needed to generate a confirmation for
/// deregistering a new collator. The implementation must match this schema:
/// https://github.com/Aventus-Network-Services/avn-bridge/blob/v1.1.0/contracts/AVNBridge.sol#L390-L391
pub(crate) fn concat_and_hash_deregistration_data(
    deregister_collator_data: &DeregisterCollatorData,
) -> [u8; 32] {
    let mut deregister_collator_params_concat: [u8; PACKED_KEYS_SIZE] = [0u8; PACKED_KEYS_SIZE];

    deregister_collator_params_concat[0..32]
        .copy_from_slice(&deregister_collator_data.t2_public_key[0..32]);
    deregister_collator_params_concat[32..PACKED_KEYS_SIZE]
        .copy_from_slice(&deregister_collator_data.t1_public_key.as_bytes()[0..64]);

    let deregister_collator_hash = keccak_256(&deregister_collator_params_concat);

    log::info!(
        "🗜️ Creating packed hash for {:?} transaction: Concat params data (hex encoded): {:?} - keccak_256 hash (hex encoded): {:?}",
            &deregister_collator_data,
            hex::encode(deregister_collator_params_concat),
            hex::encode(deregister_collator_hash)
    );
    return deregister_collator_hash
}
