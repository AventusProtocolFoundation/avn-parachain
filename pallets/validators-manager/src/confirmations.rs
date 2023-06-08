use pallet_ethereum_transactions::ethereum_transaction::{
    ActivateCollatorData, DeregisterCollatorData, EthAbiHelper, EthTransactionType, TransactionId,
};
use sp_core::H512;
use sp_io::hashing::keccak_256;

use frame_support::{dispatch::DispatchResult, log};

const PACKED_KEYS_SIZE: usize = 96;

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

    log::debug!(
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

    log::debug!(
            "🗜️ Creating packed hash for {:?} transaction: Concat params data (hex encoded): {:?} - keccak_256 hash (hex encoded): {:?}",
                &deregister_collator_data,
                hex::encode(deregister_collator_params_concat),
                hex::encode(deregister_collator_hash)
        );
    return deregister_collator_hash
}

// Tests are
#[test]
fn collator_activation_hashed_params_are_valid() {
    let mut ferdie_t1_public_key_bytes: [u8; 64] = [0; 64];

    assert!(hex::decode_to_slice("1f21d300f707014f718f41c969c054936b7a105a478da74d37ec75fa0f831f872aeb02d6af6c098e3d523cdcca8e82c13672ff083b94f4a8fc3d265a3369db20", &mut ferdie_t1_public_key_bytes[..]).is_ok());
    let mut ferdie_t2_public_key_bytes: [u8; 32] = [0; 32];
    assert!(hex::decode_to_slice(
        "e659a7a1628cdd93febc04a4e0646ea20e9f5f0ce097d9a05290d4a9e054df4e",
        &mut ferdie_t2_public_key_bytes[..]
    )
    .is_ok());

    let ferdie_activation_data = ActivateCollatorData {
        t1_public_key: H512::from(ferdie_t1_public_key_bytes),
        t2_public_key: ferdie_t2_public_key_bytes,
    };

    let hashed_keys = concat_and_hash_activation_data(&ferdie_activation_data);

    assert_eq!(
        hex::encode(&hashed_keys),
        "fcde037cef635ab9da60f50efd8552403f7a7e6e58f1f1be3aba810ff99228ea"
    );

}

#[test]
fn collator_deregistration_hashed_params_are_valid() {
    let mut ferdie_t1_public_key_bytes: [u8; 64] = [0; 64];

    assert!(hex::decode_to_slice("1f21d300f707014f718f41c969c054936b7a105a478da74d37ec75fa0f831f872aeb02d6af6c098e3d523cdcca8e82c13672ff083b94f4a8fc3d265a3369db20", &mut ferdie_t1_public_key_bytes[..]).is_ok());
    let mut ferdie_t2_public_key_bytes: [u8; 32] = [0; 32];
    assert!(hex::decode_to_slice(
        "e659a7a1628cdd93febc04a4e0646ea20e9f5f0ce097d9a05290d4a9e054df4e",
        &mut ferdie_t2_public_key_bytes[..]
    )
    .is_ok());

    let ferdie_deregistration_data = DeregisterCollatorData {
        t1_public_key: H512::from(ferdie_t1_public_key_bytes),
        t2_public_key: ferdie_t2_public_key_bytes,
    };

    let hashed_keys = concat_and_hash_deregistration_data(&ferdie_deregistration_data);

    assert_eq!(
        hex::encode(&hashed_keys),
        "5721a2e809ae9b2e3d714423df2aff325ec629783d5f559afe3e629ae491eb91"
    );
}
