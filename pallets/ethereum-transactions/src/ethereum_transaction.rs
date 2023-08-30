use codec::{Decode, Encode, MaxEncodedLen};
use sp_avn_common::EthTransaction;
use sp_core::{ecdsa, H160, H256, H512, U256};

#[cfg(not(feature = "std"))]
extern crate alloc;
use crate::{Config, Error, TypeInfo};
#[cfg(not(feature = "std"))]
use alloc::string::String;
use sp_std::prelude::*;

use ethabi::{Error as EthAbiError, Function, Param, ParamType, Token};

// ================================= Ethereum Transaction Types ====================================

#[derive(Encode, Decode, Clone, PartialEq, Debug, Eq, MaxEncodedLen, TypeInfo)]
pub enum EthTransactionType {
    PublishRoot(PublishRootData),
    #[deprecated(
        note = "Parachains use collators so this is deprecated. Use only `DeregisterCollator` instead"
    )]
    DeregisterValidator(DeregisterValidatorData),
    SlashValidator(SlashValidatorData),
    #[deprecated(
        note = "Parachains use collators so this is deprecated. Use only `ActivateCollator` instead"
    )]
    ActivateValidator(ActivateValidatorData),
    Invalid,
    Discarded(TransactionId),
    ActivateCollator(ActivateCollatorData),
    DeregisterCollator(DeregisterCollatorData),
}

impl Default for EthTransactionType {
    fn default() -> Self {
        EthTransactionType::Invalid
    }
}

impl EthTransactionType {
    pub fn to_abi(&self) -> Result<EthTransactionDescription, ethabi::Error> {
        match self {
            EthTransactionType::PublishRoot(d) => Ok(d.to_abi()),
            EthTransactionType::ActivateCollator(d) => Ok(d.to_abi()),
            EthTransactionType::DeregisterCollator(d) => Ok(d.to_abi()),
            _ => Err(EthAbiError::InvalidData),
        }
    }
}

#[derive(Encode, Decode, Default, Clone, PartialEq, Debug, Eq, MaxEncodedLen, TypeInfo)]
pub struct PublishRootData {
    pub root_hash: [u8; 32],
}

impl PublishRootData {
    pub fn new(root_hash: [u8; 32]) -> PublishRootData {
        PublishRootData { root_hash }
    }

    pub fn to_abi(&self) -> EthTransactionDescription {
        EthTransactionDescription {
            function_call: Function {
                name: String::from("publishRoot"),
                inputs: vec![Param {
                    name: String::from("_rootHash"),
                    kind: ParamType::FixedBytes(32),
                }],
                outputs: Vec::<Param>::new(),
                constant: false,
            },
            call_values: vec![Token::FixedBytes(self.root_hash.to_vec())],
        }
    }
}

#[derive(Encode, Decode, Default, Clone, PartialEq, Debug, Eq, MaxEncodedLen, TypeInfo)]
pub struct DeregisterValidatorData {
    pub t2_public_key: [u8; 32],
}

impl DeregisterValidatorData {
    pub fn new(t2_public_key: [u8; 32]) -> DeregisterValidatorData {
        DeregisterValidatorData { t2_public_key }
    }

    pub fn to_abi(&self) -> EthTransactionDescription {
        EthTransactionDescription {
            function_call: Function {
                name: String::from("deregisterValidator"),
                inputs: vec![Param {
                    name: String::from("_targetT2PublicKey"),
                    kind: ParamType::FixedBytes(32),
                }],
                outputs: Vec::<Param>::new(),
                constant: false,
            },
            call_values: vec![Token::FixedBytes(self.t2_public_key.to_vec())],
        }
    }
}

#[derive(Encode, Decode, Default, Clone, PartialEq, Debug, Eq, MaxEncodedLen, TypeInfo)]
pub struct DeregisterCollatorData {
    pub t1_public_key: H512,
    pub t2_public_key: [u8; 32],
}

impl DeregisterCollatorData {
    pub fn new(t1_public_key: H512, t2_public_key: [u8; 32]) -> DeregisterCollatorData {
        DeregisterCollatorData { t1_public_key, t2_public_key }
    }

    pub fn to_abi(&self) -> EthTransactionDescription {
        EthTransactionDescription {
            function_call: Function {
                name: String::from("deregisterValidator"),
                inputs: vec![
                    Param { name: String::from("_targetT1PublicKey"), kind: ParamType::Bytes },
                    Param {
                        name: String::from("_targetT2PublicKey"),
                        kind: ParamType::FixedBytes(32),
                    },
                ],
                outputs: Vec::<Param>::new(),
                constant: false,
            },
            call_values: vec![
                Token::Bytes(self.t1_public_key.to_fixed_bytes().to_vec()),
                Token::FixedBytes(self.t2_public_key.to_vec()),
            ],
        }
    }
}

#[derive(Encode, Decode, Default, Clone, PartialEq, Debug, Eq, MaxEncodedLen, TypeInfo)]
pub struct SlashValidatorData {
    pub t2_public_key: [u8; 32],
}

#[derive(Encode, Decode, Default, Clone, PartialEq, Debug, Eq, MaxEncodedLen, TypeInfo)]
pub struct ActivateValidatorData {
    pub t2_public_key: [u8; 32],
}

impl ActivateValidatorData {
    pub fn new(t2_public_key: [u8; 32]) -> ActivateValidatorData {
        ActivateValidatorData { t2_public_key }
    }
}

#[derive(Encode, Decode, Default, Clone, PartialEq, Debug, Eq, MaxEncodedLen, TypeInfo)]
pub struct ActivateCollatorData {
    pub t1_public_key: H512,
    pub t2_public_key: [u8; 32],
}

impl ActivateCollatorData {
    pub fn new(t1_public_key: H512, t2_public_key: [u8; 32]) -> ActivateCollatorData {
        ActivateCollatorData { t1_public_key, t2_public_key }
    }

    pub fn to_abi(&self) -> EthTransactionDescription {
        EthTransactionDescription {
            function_call: Function {
                name: String::from("registerValidator"),
                inputs: vec![
                    Param { name: String::from("_targetT1PublicKey"), kind: ParamType::Bytes },
                    Param {
                        name: String::from("_targetT2PublicKey"),
                        kind: ParamType::FixedBytes(32),
                    },
                ],
                outputs: Vec::<Param>::new(),
                constant: false,
            },
            call_values: vec![
                Token::Bytes(self.t1_public_key.to_fixed_bytes().to_vec()),
                Token::FixedBytes(self.t2_public_key.to_vec()),
            ],
        }
    }
}

pub type TransactionId = u64;
pub type EthereumTransactionHash = H256;

#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo)]
pub struct EthTransactionCandidate {
    pub tx_id: TransactionId,
    pub from: Option<[u8; 32]>, // AvN Public Key of a validator
    pub call_data: EthTransactionType,
    pub signatures: EthSignatures,
    pub quorum: u32, // number of signatures needed

    // TODO [TYPE: business logic][PRI: high] this needs review since we allow locks to expire, it
    // is possible to send the same transaction twice and then we need to update this value.
    // Example: we create a lock to send a transaction to Ethereum. We send it and receive a
    // temporary `eth_tx_hash` The transaction later becomes stale and has to be resent.
    // But because tx_hash can only be set once, we will not be able to update it as is.

    // eth_tx_hash should be None and set only once, with an ethereum transaction
    // hash.EthTransactionCandidate Should we run in a situation that we try to overwrite a
    // value, then an error should be raised, logged and investigated To enforce this, we make
    // this a private field with set/get functions that will throw errors. Additionally since
    // we can't have a Option<H256> in Storage, we map H256::zero() with None in the get function.
    eth_tx_hash: EthereumTransactionHash,
}

impl EthTransactionCandidate {
    pub fn ready_to_dispatch(&self) -> bool {
        return self.from.is_some() && self.signatures.count() >= self.quorum
    }

    pub fn new(
        tx_id: TransactionId,
        from: Option<[u8; 32]>,
        call_data: EthTransactionType,
        quorum: u32,
    ) -> EthTransactionCandidate {
        EthTransactionCandidate {
            from,
            call_data,
            tx_id,
            signatures: EthSignatures::new(),
            quorum,
            eth_tx_hash: H256::zero(),
        }
    }

    // TODO [TYPE: refactoring][PRI: medium]: We have multiple `to_abi()` methods that return
    // different things, we should rename this one to `to_abi_encoding()`
    pub fn to_abi(&self, ethereum_contract: H160) -> Result<EthTransaction, ethabi::Error> {
        let from = self.from.clone().unwrap();
        let transaction_description = EthAbiHelper::generate_full_ethereum_description(
            &self.call_data,
            self.tx_id,
            &self.signatures,
        )?;
        EthAbiHelper::generate_ethereum_transaction_abi(
            from,
            ethereum_contract,
            &transaction_description,
        )
    }

    pub fn get_eth_tx_hash(&self) -> Option<EthereumTransactionHash> {
        if self.eth_tx_hash == H256::zero() {
            return None
        }
        Some(self.eth_tx_hash)
    }

    pub fn set_eth_tx_hash<T: Config>(&mut self, new_hash: H256) -> Result<(), Error<T>> {
        // If the input is zero, it either changes 0 to 0, or resets the hash directly
        if self.eth_tx_hash != H256::zero() && new_hash != H256::zero() {
            return Err(Error::<T>::EthTransactionHashValueMutableOnce)
        }

        self.eth_tx_hash = new_hash;
        Ok(())
    }
}

#[derive(Debug, PartialEq)]
pub struct EthTransactionDescription {
    pub function_call: ethabi::Function,
    pub call_values: Vec<Token>,
}

pub struct EthAbiHelper {}
// Avn compatible ethereum methods must take a `_t2TransactionId` and `_confirmations` as the last 2
// parameters
// `_confirmations` is a vector of signatures of all the method parameters followed by
// `_t2TransactionId` and `_t2PublicKey`

impl EthAbiHelper {
    // TODO [TYPE: refactoring][PRI: medium]: migrate this function to avn-common, together with its
    // tests
    pub fn u256_to_big_endian(value: &U256) -> [u8; 32] {
        let mut uint256 = [0u8; 32];
        value.to_big_endian(&mut uint256[..]);
        uint256
    }

    pub fn generate_eth_abi_encoding(
        call: &EthTransactionDescription,
    ) -> Result<Vec<u8>, ethabi::Error> {
        call.function_call.encode_input(&call.call_values)
    }

    pub fn generate_ethereum_description(
        call: &EthTransactionType,
        transaction_id: TransactionId,
    ) -> Result<EthTransactionDescription, ethabi::Error> {
        let mut mut_call = call.to_abi()?;
        // All targeted ethereum calls should have these fields at the end.
        mut_call.function_call.inputs.append(&mut vec![Param {
            name: String::from("_t2TransactionId"),
            kind: ParamType::Uint(256),
        }]);
        mut_call.call_values.push(Token::Uint(
            EthAbiHelper::u256_to_big_endian(&U256::from(transaction_id)).into(),
        ));
        Ok(mut_call)
    }

    pub fn generate_full_ethereum_description(
        call: &EthTransactionType,
        transaction_id: TransactionId,
        signatures: &EthSignatures,
    ) -> Result<EthTransactionDescription, ethabi::Error> {
        let mut mut_call = EthAbiHelper::generate_ethereum_description(call, transaction_id)?;
        // All targeted ethereum calls should have this field at the end.
        mut_call.function_call.inputs.append(&mut vec![Param {
            name: String::from("_confirmations"),
            kind: ParamType::Bytes,
        }]);
        mut_call.call_values.push(Token::Bytes(signatures.to_bytes()));
        Ok(mut_call)
    }

    pub fn encode_arguments(
        hash_data: &[u8; 32],
        expiry: U256,
        transaction_id: TransactionId,
    ) -> Vec<u8> {
        let call_values: Vec<Token> = vec![
            Token::FixedBytes(hash_data.to_vec()),
            Token::Uint(EthAbiHelper::u256_to_big_endian(&expiry).into()),
            Token::Uint(EthAbiHelper::u256_to_big_endian(&U256::from(transaction_id)).into()),
        ];
        return ethabi::encode(&call_values)
    }

    // this is only for validators manager and will be deleted when we focus on that pallet
    pub fn generate_ethereum_abi_data_for_signature_request(
        hash_data: &[u8; 32],
        transaction_id: TransactionId,
        from: &[u8; 32],
    ) -> Vec<u8> {
        let call_values: Vec<Token> = vec![
            Token::FixedBytes(hash_data.to_vec()),
            Token::Uint(EthAbiHelper::u256_to_big_endian(&U256::from(transaction_id)).into()),
            Token::FixedBytes(from.to_vec()),
        ];
        return ethabi::encode(&call_values)
    }

    pub fn generate_ethereum_transaction_abi(
        from: [u8; 32],
        to: H160,
        transaction_description: &EthTransactionDescription,
    ) -> Result<EthTransaction, ethabi::Error> {
        let encoded_data = EthAbiHelper::generate_eth_abi_encoding(transaction_description)?;
        Ok(EthTransaction::new(from, to, encoded_data))
    }
}

#[derive(PartialEq, Eq, Debug, TypeInfo)]
pub enum OtherError {
    InvalidEcdsaData,
    DuplicateSignature,
}

// TODO [TYPE: refactoring][PRI: medium][JIRA: 92]: replace by some Substrate inbuilt ECDSA type
// but keep the code for validation of the bytes

// Ethereum-like ECDSA signatures are 65 bytes long. std::array::LengthAtMost32 trait does now allow
// us to have the signatures stored as bytes array, so we are encapsulating this in a struct
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo)]
pub struct EcdsaSignature {
    // first 32 bytes, after the length prefix
    pub r: [u8; 32],
    // second 32 bytes
    pub s: [u8; 32],
    // final byte (first byte of the next 32 bytes)
    pub v: [u8; 1],
}

impl EcdsaSignature {
    pub fn new(raw_sig: [u8; 65]) -> Result<EcdsaSignature, OtherError> {
        let mut r: [u8; 32] = Default::default();
        r.copy_from_slice(&raw_sig[0..32]);

        if r == [0; 32] {
            return Err(OtherError::InvalidEcdsaData)
        }

        let mut s: [u8; 32] = Default::default();
        s.copy_from_slice(&raw_sig[32..64]);
        if s == [0; 32] {
            return Err(OtherError::InvalidEcdsaData)
        }

        let mut v: [u8; 1] = Default::default();
        v.copy_from_slice(&raw_sig[64..]);
        if !(v == [0] || v == [1] || v == [27] || v == [28]) {
            return Err(OtherError::InvalidEcdsaData)
        }

        return Ok(EcdsaSignature { r, s, v })
    }

    pub fn to_vec(&self) -> Vec<u8> {
        let mut data = Vec::<u8>::new();
        data.append(&mut self.r.to_vec());
        data.append(&mut self.s.to_vec());
        data.append(&mut self.v.to_vec());
        data
    }
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo)]
pub struct EthSignatures {
    // TODO [TYPE: refactoring][PRI: medium]: make this private and fix tests.rs to deal with it
    // Alternatively: consider removing this struct entirely
    pub signatures_list: Vec<ecdsa::Signature>,
}

impl EthSignatures {
    pub fn count(&self) -> u32 {
        return self.signatures_list.len() as u32
    }

    pub fn append(&mut self, mut other: Vec<ecdsa::Signature>) {
        self.signatures_list.append(&mut other);
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut data = Vec::<u8>::new();
        for sig in self.signatures_list.iter() {
            let bytes: [u8; 65] = *sig.as_ref();
            data.append(&mut bytes.to_vec());
        }
        data
    }

    pub fn add(&mut self, signature: ecdsa::Signature) -> Result<(), OtherError> {
        // tentative change: logically we don't welcome duplicate signatures here
        // but does this have a hit in performance that will be caught later or is this checked
        // elsewhere?
        if self.signatures_list.contains(&signature) {
            return Err(OtherError::DuplicateSignature)
        }
        self.signatures_list.push(signature);
        Ok(())
    }

    pub fn new() -> Self {
        return EthSignatures { signatures_list: Vec::<ecdsa::Signature>::new() }
    }
}

// ================================= Authorities and Validators
// =======================================
pub type AuthIndex = u32;
