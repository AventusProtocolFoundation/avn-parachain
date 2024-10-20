#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::string::String;

use codec::{Codec, Decode, Encode};
use sp_core::{crypto::KeyTypeId, ecdsa, H160, H256};
use sp_io::{crypto::secp256k1_ecdsa_recover_compressed, hashing::keccak_256, EcdsaVerifyError};
use sp_runtime::{
    scale_info::TypeInfo,
    traits::{AtLeast32Bit, Dispatchable, IdentifyAccount, Member, Verify},
};
use sp_std::{boxed::Box, vec::Vec};

pub const OPEN_BYTES_TAG: &'static [u8] = b"<Bytes>";
pub const CLOSE_BYTES_TAG: &'static [u8] = b"</Bytes>";

#[path = "tests/helpers.rs"]
pub mod avn_tests_helpers;
pub mod eth_key_actions;
pub mod event_discovery;
pub mod event_types;
pub mod ocw_lock;

/// Ingress counter type for a counter that can sign the same message with a different signature
/// each time
pub type IngressCounter = u64;

/// Key type for AVN pallet. dentified as `avnk`.
pub const AVN_KEY_ID: KeyTypeId = KeyTypeId(*b"avnk");
/// Key type for signing ethereum compatible signatures, built-in. Identified as `ethk`.
pub const ETHEREUM_SIGNING_KEY: KeyTypeId = KeyTypeId(*b"ethk");
/// Ethereum prefix
pub const ETHEREUM_PREFIX: &'static [u8] = b"\x19Ethereum Signed Message:\n32";

/// Local storage key to access the external service's port number
pub const EXTERNAL_SERVICE_PORT_NUMBER_KEY: &'static [u8; 15] = b"avn_port_number";
/// Default port number the external service runs on.
pub const DEFAULT_EXTERNAL_SERVICE_PORT_NUMBER: &str = "2020";

pub mod bounds {
    use sp_core::ConstU32;

    /// Bound used for Vectors containing validators
    pub type MaximumValidatorsBound = ConstU32<256>;
    /// Bound used for voting session IDs
    pub type VotingSessionIdBound = ConstU32<64>;
    /// Bound used for NFT external references
    pub type NftExternalRefBound = ConstU32<1024>;
}

#[derive(Debug)]
pub enum ECDSAVerificationError {
    InvalidSignature,
    InvalidValueForV,
    InvalidValueForRS,
    InvalidMessageFormat,
    BadSignature,
}

// Struct that holds the information about an Ethereum transaction
// See https://github.com/ethereum/wiki/wiki/JSON-RPC#parameters-22
#[derive(Encode, Decode, Clone, PartialEq, Debug, Eq, Default)]
pub struct EthTransaction {
    pub from: [u8; 32],
    pub to: H160,
    pub data: Vec<u8>,
}

impl EthTransaction {
    pub fn new(from: [u8; 32], to: H160, data: Vec<u8>) -> Self {
        return EthTransaction { from, to, data }
    }
}

#[derive(Encode, Decode, Clone, PartialEq, Debug, Eq, Default)]
pub struct NewEthTransaction {
    pub from: [u8; 32],
    pub to: H160,
    pub data: Vec<u8>,
    pub block: Option<u32>,
    pub period_id: Option<u32>,
}

impl NewEthTransaction {
    pub fn new(from: [u8; 32], to: H160, data: Vec<u8>, block: Option<u32>, period_id: Option<u32>) -> Self {
        return NewEthTransaction { from, to, data, block, period_id }
    }
}

#[derive(Encode, Decode, Copy, Clone, PartialEq, Eq, Default, Debug, TypeInfo)]
pub struct Proof<Signature: TypeInfo, AccountId> {
    pub signer: AccountId,
    pub relayer: AccountId,
    pub signature: Signature,
}

pub trait CallDecoder {
    // The type that represents an account id defined in the trait (T::AccountId)
    type AccountId;

    // The type that represents a signature
    type Signature: TypeInfo;

    // The type used to throw an error (Error<T>)
    type Error;

    /// The type which encodes the call to be decoded.
    type Call;

    fn get_proof(call: &Self::Call)
        -> Result<Proof<Self::Signature, Self::AccountId>, Self::Error>;
}

// ======================================== Proxy validation
// ==========================================

pub trait InnerCallValidator {
    type Call: Dispatchable;

    fn signature_is_valid(_call: &Box<Self::Call>) -> bool {
        false
    }
}

pub trait FeePaymentHandler {
    // The type that represents an account id defined in the trait (T::AccountId)
    type AccountId;
    // The type that represents a non native token balance
    type TokenBalance;
    // The type that represents a non native token identifier
    type Token;
    // The type used to throw an error (Error<T>)
    type Error;

    fn pay_fee(
        _token: &Self::Token,
        _amount: &Self::TokenBalance,
        _payer: &Self::AccountId,
        _recipient: &Self::AccountId,
    ) -> Result<(), Self::Error>;
}

impl FeePaymentHandler for () {
    type Token = ();
    type TokenBalance = ();
    type AccountId = ();
    type Error = ();

    fn pay_fee(
        _token: &Self::Token,
        _amount: &Self::TokenBalance,
        _payer: &Self::AccountId,
        _recipient: &Self::AccountId,
    ) -> Result<(), ()> {
        Err(())
    }
}

pub fn safe_add_block_numbers<BlockNumber: Member + Codec + AtLeast32Bit>(
    left: BlockNumber,
    right: BlockNumber,
) -> Result<BlockNumber, ()> {
    Ok(left.checked_add(&right).ok_or(())?.into())
}

pub fn safe_sub_block_numbers<BlockNumber: Member + Codec + AtLeast32Bit>(
    left: BlockNumber,
    right: BlockNumber,
) -> Result<BlockNumber, ()> {
    Ok(left.checked_sub(&right).ok_or(())?.into())
}

pub fn recover_public_key_from_ecdsa_signature(
    signature: &ecdsa::Signature,
    message: &String,
) -> Result<ecdsa::Public, ECDSAVerificationError> {
    match secp256k1_ecdsa_recover_compressed(
        signature.as_ref(),
        &hash_with_ethereum_prefix(message)?,
    ) {
        Ok(pubkey) => return Ok(ecdsa::Public::from_raw(pubkey)),
        Err(EcdsaVerifyError::BadRS) => return Err(ECDSAVerificationError::InvalidValueForRS),
        Err(EcdsaVerifyError::BadV) => return Err(ECDSAVerificationError::InvalidValueForV),
        Err(EcdsaVerifyError::BadSignature) => return Err(ECDSAVerificationError::BadSignature),
    }
}

pub fn hash_with_ethereum_prefix(hex_message: &String) -> Result<[u8; 32], ECDSAVerificationError> {
    let message_bytes = hex::decode(hex_message.trim_start_matches("0x"))
        .map_err(|_| ECDSAVerificationError::InvalidMessageFormat)?;

    let mut prefixed_message = ETHEREUM_PREFIX.to_vec();
    prefixed_message.append(&mut message_bytes.to_vec());
    let hash = keccak_256(&prefixed_message);
    log::debug!(
        "🪲 Data without prefix: {:?},\n data with ethereum prefix: {:?}, \n result hash: {:?}",
        &hex_message,
        &prefixed_message,
        hex::encode(&hash),
    );
    Ok(hash)
}

pub fn verify_signature<Signature: Member + Verify + TypeInfo, AccountId: Member>(
    proof: &Proof<Signature, <<Signature as Verify>::Signer as IdentifyAccount>::AccountId>,
    signed_payload: &[u8],
) -> Result<(), ()> {
    let wrapped_signed_payload: Vec<u8> =
        [OPEN_BYTES_TAG, signed_payload, CLOSE_BYTES_TAG].concat();
    match proof.signature.verify(&*wrapped_signed_payload, &proof.signer) {
        true => Ok(()),
        false => match proof.signature.verify(signed_payload, &proof.signer) {
            true => Ok(()),
            false => Err(()),
        },
    }
}

#[derive(Encode, Decode, Clone, PartialEq, Debug, Eq)]
pub struct EthQueryRequest {
    pub tx_hash: H256,
    pub response_type: EthQueryResponseType,
}

impl EthQueryRequest {
    pub fn new(tx_hash: H256, response_type: EthQueryResponseType) -> Self {
        return EthQueryRequest { tx_hash, response_type }
    }
}

#[derive(Encode, Decode, Clone, PartialEq, Debug, Eq)]
pub enum EthQueryResponseType {
    CallData,
    TransactionReceipt,
}

#[derive(Encode, Decode, Clone, PartialEq, Debug, Eq)]
pub struct EthQueryResponse {
    pub data: Vec<u8>,
    pub num_confirmations: u64,
}
