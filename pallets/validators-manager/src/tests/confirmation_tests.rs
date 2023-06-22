use crate::{concat_and_hash_activation_data, concat_and_hash_deregistration_data};
use hex_literal::hex;
use pallet_ethereum_transactions::ethereum_transaction::{
    ActivateCollatorData, DeregisterCollatorData, EthAbiHelper,
};

/** JS code used to generate the const values
const { ethers } = require('ethers');
const EC = require('elliptic').ec;
const ec = new EC('secp256k1');

async function main() {
  const collator_action_T1_compressed_public_key = '0x03471b4c1012dddf4d494c506a098c7b1b719b20bbb177b1174f2166f953c29503';
  const collator_action_T1_public_key =
    '0x' + ec.keyFromPublic(collator_action_T1_compressed_public_key.slice(2), 'hex').getPublic().encode('hex').slice(2);
  const collator_action_T2_public_key = '0x90b5ab205c6974c9ea841be688864633dc9ca8a357843eeacf2314649965fe22';
  const sender_T2_public_key = '0x1cbd2d43530a44705ad088af313e18f80b53ef16b36177cd4b77b846f2a5f07c';

  let t2_transactionID = 100;
  const registrationHash = ethers.utils.solidityKeccak256(
    ['bytes', 'bytes32'],
    [collator_action_T1_public_key, collator_action_T2_public_key]
  );
  registrationEncodedParams = ethers.utils.defaultAbiCoder.encode(
    ['bytes32', 'uint256', 'bytes32'],
    [registrationHash, t2_transactionID, sender_T2_public_key]
  );
  const deregistrationHash = ethers.utils.solidityKeccak256(
    ['bytes32', 'bytes'],
    [collator_action_T2_public_key, collator_action_T1_public_key]
  );
  let deregistrationEncodedParams = ethers.utils.defaultAbiCoder.encode(
    ['bytes32', 'uint256', 'bytes32'],
    [deregistrationHash, t2_transactionID, sender_T2_public_key]
  );

  console.log(`pub(crate) const COLLATOR_ACTION_ETHEREUM_PUBLIC_KEY: [u8; 64] =
    hex!["${collator_action_T1_public_key.slice(2)}"];`);
  console.log(`pub(crate) const COLLATOR_ACTION_AVN_PUBLIC_KEY: [u8; 32] =
    hex!["${collator_action_T2_public_key.slice(2)}"];`);
  console.log(`pub(crate) const TEST_TRANSACTION_ID: u64 = ${t2_transactionID};`);
  console.log(`pub(crate) const CONCAT_REGISTRATION_HASH: [u8; 32] =
    hex!["${registrationHash.slice(2)}"];`);
  console.log(`pub(crate) const SENDER_T2_PUBLIC_KEY: [u8; 32] =
    hex!["${sender_T2_public_key.slice(2)}"];`);
  console.log(`pub(crate) const CONCAT_DEREGISTRATION_HASH: [u8; 32] =
    hex!["${deregistrationHash.slice(2)}"];`);
  console.log(`pub(crate) const ENCODED_REGISTRATION_CONFIRMATION_ABI: &'static str =
    "${registrationEncodedParams.slice(2)}";`);
  console.log(`pub(crate) const ENCODED_DEREGISTRATION_CONFIRMATION_ABI: &'static str =
    "${deregistrationEncodedParams.slice(2)}";`);
}

main();
*/

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
