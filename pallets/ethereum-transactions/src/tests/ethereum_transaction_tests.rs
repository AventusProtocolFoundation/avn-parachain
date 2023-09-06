#![cfg(test)]
#![allow(unused_must_use)]

use ethabi::{Param, Token};
use frame_support::assert_err;
use sp_core::{ecdsa, U256};

use crate::{ethereum_transaction::*, tests_eth_transaction_type::*};

// EcdsaSignature tests
fn dummy_ecdsa_signature_as_bytes(r: [u8; 32], s: [u8; 32], v: [u8; 1]) -> [u8; 65] {
    let mut sig = Vec::new();
    sig.extend_from_slice(&r);
    sig.extend_from_slice(&s);
    sig.extend_from_slice(&v);

    let mut result = [0; 65];
    result.copy_from_slice(&sig[..]);
    return result
}

fn generate_dummy_ecdsa_signature(i: u8) -> ecdsa::Signature {
    let mut bytes: [u8; 65] = [0; 65];
    let first_64_bytes: [u8; 64] = [i; 64];
    bytes[0..64].copy_from_slice(&first_64_bytes);
    return ecdsa::Signature::from_raw(bytes)
}

#[test]
fn ecdsa_signature_to_vec_works() {
    let r = [1; 32];
    let s = [2; 32];
    let v = [28];

    let sig = EcdsaSignature { r, s, v };

    let bytes = sig.to_vec();
    let mut expected_bytes = Vec::new();
    expected_bytes.extend_from_slice(&dummy_ecdsa_signature_as_bytes(r, s, v));

    assert_eq!(bytes, expected_bytes);
}

#[test]
fn parse_valid_ecdsa_signature_succeeds() {
    let r = [1; 32];
    let s = [2; 32];

    let v_legal_values = &[0u8, 1u8, 27u8, 28u8];

    for v_slice in v_legal_values {
        let v: [u8; 1] = [*v_slice];
        let test_data = dummy_ecdsa_signature_as_bytes(r, s, v);
        let sig = EcdsaSignature::new(test_data).unwrap();

        assert_eq!(sig.r, r);
        assert_eq!(sig.s, s);
        assert_eq!(sig.v, v);
    }
}

#[test]
fn parse_ecdsa_signature_fails_when_r_is_zero() {
    let r = [0; 32];
    let s = [2; 32];
    let v = [27];

    let test_data = dummy_ecdsa_signature_as_bytes(r, s, v);

    assert_err!(EcdsaSignature::new(test_data), OtherError::InvalidEcdsaData);
}

#[test]
fn parse_ecdsa_signature_fails_when_s_is_zero() {
    let r = [1; 32];
    let s = [0; 32];
    let v = [27];

    let test_data = dummy_ecdsa_signature_as_bytes(r, s, v);

    assert_err!(EcdsaSignature::new(test_data), OtherError::InvalidEcdsaData);
}

#[test]
fn parse_ecdsa_signature_fails_when_v_is_invalid() {
    let r = [1; 32];
    let s = [2; 32];

    // test some invalid v values

    let test_data = dummy_ecdsa_signature_as_bytes(r, s, [3]);
    assert_err!(EcdsaSignature::new(test_data), OtherError::InvalidEcdsaData);

    let test_data = dummy_ecdsa_signature_as_bytes(r, s, [55]);
    assert_err!(EcdsaSignature::new(test_data), OtherError::InvalidEcdsaData);

    let test_data = dummy_ecdsa_signature_as_bytes(r, s, [247]);
    assert_err!(EcdsaSignature::new(test_data), OtherError::InvalidEcdsaData);
}

// EthTransactionCandidate tests
fn good_transaction_candidate() -> EthTransactionCandidate {
    let mut tx = EthTransactionCandidate::new(
        1u64,
        Some([1; 32]),
        EthTransactionType::PublishRoot(generate_publish_root_data(ROOT_HASH)),
        1,
    );

    tx.signatures.add(generate_dummy_ecdsa_signature(1));
    return tx
}

#[test]
fn ready_to_dispatch_success_case() {
    let tx = good_transaction_candidate();

    assert!(tx.ready_to_dispatch());
}

#[test]
fn ready_to_dispatch_fails_with_insufficient_signatures() {
    let mut tx = good_transaction_candidate();

    tx.quorum = 4;
    tx.signatures.add(generate_dummy_ecdsa_signature(2));
    tx.signatures.add(generate_dummy_ecdsa_signature(3));
    assert_eq!(tx.signatures.count(), 3);

    assert!(!tx.ready_to_dispatch());
}

#[test]
fn ready_to_dispatch_fails_if_from_is_none() {
    let mut tx = good_transaction_candidate();
    tx.from = None;

    assert!(!tx.ready_to_dispatch());
}

// TODO [TYPE: tests][PRI: low]: test to_abi in this struct

// EthAbiHelper tests

#[test]
fn u256_to_big_endian_success_cases() {
    let a = U256::one();
    let b = U256::from(16u8);
    let c = U256::from((10 << 8) + (20 << 16) + (30 << 24));
    let z = U256::zero();
    let m = U256::MAX;

    let mut a_bytes = [0; 32];
    a_bytes[31] = 1;
    assert_eq!(EthAbiHelper::u256_to_big_endian(&a), a_bytes);

    let mut b_bytes = [0; 32];
    b_bytes[31] = 16;
    assert_eq!(EthAbiHelper::u256_to_big_endian(&b), b_bytes);

    let mut c_bytes = [0; 32];
    c_bytes[30] = 10;
    c_bytes[29] = 20;
    c_bytes[28] = 30;
    assert_eq!(EthAbiHelper::u256_to_big_endian(&c), c_bytes);

    assert_eq!(EthAbiHelper::u256_to_big_endian(&z), [0; 32]);
    assert_eq!(EthAbiHelper::u256_to_big_endian(&m), [255; 32]);
}

// TODO [TYPE: tests][PRI: low]: test abi encoding --> this looks like it is the general encoding
// function I think it is best to test the public facing functions that call this one instead,
// that is, the encoding of a specific call of a given type

fn extract_data_from_abi_description(
    tx_desc: EthTransactionDescription,
) -> (usize, Vec<Param>, Vec<Token>) {
    let inputs = tx_desc.function_call.inputs;
    let values = tx_desc.call_values;
    assert_eq!(inputs.len(), values.len());
    return (inputs.len(), inputs, values)
}

#[test]
fn full_ethereum_description_appends_signatures() {
    let mt_data = generate_publish_root_data(ROOT_HASH);
    let call = EthTransactionType::PublishRoot(mt_data);

    let tx_id = 12;
    let original_abi_description =
        EthAbiHelper::generate_ethereum_description(&call, tx_id).unwrap();
    let (number_of_original_terms, original_inputs, original_values) =
        extract_data_from_abi_description(original_abi_description);

    let mut signatures = EthSignatures::new();
    signatures.add(generate_dummy_ecdsa_signature(1));
    signatures.add(generate_dummy_ecdsa_signature(2));

    let abi_description =
        EthAbiHelper::generate_full_ethereum_description(&call, tx_id, &signatures).unwrap();
    let (number_of_terms, inputs, values) = extract_data_from_abi_description(abi_description);

    assert_eq!(&inputs[0..number_of_original_terms], &original_inputs[..]);
    assert_eq!(&values[0..number_of_original_terms], &original_values[..]);
    assert_eq!(inputs[number_of_original_terms].name, "_confirmations");
    assert_eq!(values[number_of_original_terms], Token::Bytes(signatures.to_bytes()));
    assert_eq!(number_of_terms - number_of_original_terms, 1);
}

// EthSignature tests

#[test]
fn new_method_returns_empty_vector() {
    let signatures = EthSignatures::new();

    assert_eq!(signatures.count(), 0);
}

#[test]
fn to_bytes_succeeds_for_empty_signatures() {
    let signatures = EthSignatures::new();
    let empty_vector = Vec::<u8>::new();

    assert_eq!(signatures.to_bytes(), empty_vector);
}

#[test]
fn to_bytes_succeeds_for_3_signatures() {
    let mut signatures = EthSignatures::new();
    signatures.add(generate_dummy_ecdsa_signature(1));
    signatures.add(generate_dummy_ecdsa_signature(2));
    signatures.add(generate_dummy_ecdsa_signature(3));

    let mut expected_vector = Vec::<u8>::new();
    expected_vector.extend_from_slice(&[1; 64]);
    expected_vector.push(0);

    expected_vector.extend_from_slice(&[2; 64]);
    expected_vector.push(0);

    expected_vector.extend_from_slice(&[3; 64]);
    expected_vector.push(0);

    assert_eq!(signatures.to_bytes(), expected_vector);
}

#[test]
fn adding_duplicate_signatures_should_fail() {
    let mut signatures = EthSignatures::new();
    let this_signature = generate_dummy_ecdsa_signature(1);
    let same_signature = generate_dummy_ecdsa_signature(1);
    signatures.add(this_signature);
    assert_err!(signatures.add(same_signature), OtherError::DuplicateSignature);
}

#[test]
fn adding_distinct_signatures_succeeds() {
    let mut signatures = EthSignatures::new();
    let this_signature = generate_dummy_ecdsa_signature(1);
    let another_signature = generate_dummy_ecdsa_signature(2);
    signatures.add(this_signature);
    signatures.add(another_signature);

    assert_eq!(signatures.count(), 2);
}
