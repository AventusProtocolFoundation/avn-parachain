use crate::{concat_and_hash_activation_data, concat_and_hash_deregistration_data};
use hex_literal::hex;

pub(crate) const COLLATOR_ACTION_ETHEREUM_PUBLIC_KEY: [u8; 64] =
    hex!["471b4c1012dddf4d494c506a098c7b1b719b20bbb177b1174f2166f953c295038374f56e5f37976f1007355fed023c68cc2961c1168ede681891c0706e7cd2d3"];
pub(crate) const COLLATOR_ACTION_AVN_PUBLIC_KEY: [u8; 32] =
    hex!["90b5ab205c6974c9ea841be688864633dc9ca8a357843eeacf2314649965fe22"];
pub(crate) const TEST_TRANSACTION_ID: u64 = 100;
pub(crate) const CONCAT_REGISTRATION_HASH: [u8; 32] =
    hex!["85aea9cf5353584a917b60f815bc69afc7fcf818096a47587e683b307db55c0c"];
pub(crate) const SENDER_T2_PUBLIC_KEY: [u8; 32] =
    hex!["1cbd2d43530a44705ad088af313e18f80b53ef16b36177cd4b77b846f2a5f07c"];
pub(crate) const CONCAT_DEREGISTRATION_HASH: [u8; 32] =
    hex!["554ccd4ea620013f8bdc8d78c9f36de3da5a2f713d2731ea88aa105eb5548ecf"];
pub(crate) const ENCODED_REGISTRATION_CONFIRMATION_ABI: &'static str =
    "85aea9cf5353584a917b60f815bc69afc7fcf818096a47587e683b307db55c0c00000000000000000000000000000000000000000000000000000000000000641cbd2d43530a44705ad088af313e18f80b53ef16b36177cd4b77b846f2a5f07c";
pub(crate) const ENCODED_DEREGISTRATION_CONFIRMATION_ABI: &'static str =
    "554ccd4ea620013f8bdc8d78c9f36de3da5a2f713d2731ea88aa105eb5548ecf00000000000000000000000000000000000000000000000000000000000000641cbd2d43530a44705ad088af313e18f80b53ef16b36177cd4b77b846f2a5f07c";

#[test]
fn collator_activation_hashed_params_are_valid() {
    use sp_core::H512;

    let ferdie_activation_data = ActivateCollatorData {
        t1_public_key: H512::from(COLLATOR_ACTION_ETHEREUM_PUBLIC_KEY),
        t2_public_key: COLLATOR_ACTION_AVN_PUBLIC_KEY,
    };
    let hashed_keys = concat_and_hash_activation_data(&ferdie_activation_data);

    assert_eq!(hex::encode(hashed_keys), hex::encode(CONCAT_REGISTRATION_HASH));
}

#[test]
fn collator_deregistration_hashed_params_are_valid() {
    use sp_core::H512;

    let ferdie_deregistration_data = DeregisterCollatorData {
        t1_public_key: H512::from(COLLATOR_ACTION_ETHEREUM_PUBLIC_KEY),
        t2_public_key: COLLATOR_ACTION_AVN_PUBLIC_KEY,
    };
    let hashed_keys = concat_and_hash_deregistration_data(&ferdie_deregistration_data);

    assert_eq!(hex::encode(hashed_keys), hex::encode(CONCAT_DEREGISTRATION_HASH));
}

#[test]
fn collator_registration_confirmation_abi_is_valid() {
    assert_eq!(
        ENCODED_REGISTRATION_CONFIRMATION_ABI,
        hex::encode(EthAbiHelper::generate_ethereum_abi_data_for_signature_request(
            &CONCAT_REGISTRATION_HASH,
            TEST_TRANSACTION_ID,
            &SENDER_T2_PUBLIC_KEY
        ))
    );
}

#[test]
fn collator_deregistration_confirmation_abi_is_valid() {
    assert_eq!(
        ENCODED_DEREGISTRATION_CONFIRMATION_ABI,
        hex::encode(EthAbiHelper::generate_ethereum_abi_data_for_signature_request(
            &CONCAT_DEREGISTRATION_HASH,
            TEST_TRANSACTION_ID,
            &SENDER_T2_PUBLIC_KEY
        ))
    );
}
