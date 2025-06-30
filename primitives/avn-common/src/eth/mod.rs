#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::string::String;
#[cfg(not(feature = "std"))]
use alloc::{string::String, vec::Vec};

use alloy_primitives::{Address, Bytes, FixedBytes, B256 as AlloyB256, U256 as AlloyU256};
use alloy_sol_types::{eip712_domain, sol, Eip712Domain, SolStruct};
use codec::{Decode, Encode, MaxEncodedLen};
use core::str;
use sp_core::{ConstU32, H160, H256};
use sp_io::hashing::blake2_256;
use sp_runtime::{scale_info::TypeInfo, BoundedVec};
use sp_std::vec::Vec;
pub type EthereumId = u32;

pub const PACKED_LOWER_PARAM_SIZE: usize = 76;
pub type LowerParams = [u8; PACKED_LOWER_PARAM_SIZE];

#[derive(Encode, Decode, Default, Clone, Debug, PartialEq, Eq, TypeInfo, MaxEncodedLen)]
pub enum EthereumNetwork {
    #[default]
    Ethereum,
    EWC,
    Sepolia,
    Holesky,
    Volta,
    Custom(u64),
}

impl EthereumNetwork {
    pub fn chain_id(&self) -> u64 {
        match self {
            EthereumNetwork::Ethereum => 1_u64,
            EthereumNetwork::EWC => 246_u64,
            EthereumNetwork::Sepolia => 11155111_u64,
            EthereumNetwork::Holesky => 17000_u64,
            EthereumNetwork::Volta => 73799_u64,
            EthereumNetwork::Custom(value) => *value,
        }
    }
}

#[derive(Encode, Decode, Default, Clone, Debug, PartialEq, Eq, TypeInfo, MaxEncodedLen)]
pub struct EthBridgeInstance {
    pub network: EthereumNetwork,
    pub bridge_contract: H160,
    pub name: BoundedVec<u8, ConstU32<256>>,
    pub version: BoundedVec<u8, ConstU32<256>>,
    pub salt: Option<[u8; 32]>,
}

impl EthBridgeInstance {
    pub fn hash(&self) -> [u8; 32] {
        let encoded = self.encode();
        blake2_256(&encoded)
    }
}

impl Into<Eip712Domain> for EthBridgeInstance {
    fn into(self) -> Eip712Domain {
        let name_vec: Vec<u8> = self.name.into_inner();
        let version_vec: Vec<u8> = self.version.into_inner();

        // Ignore salt for now
        eip712_domain!(
            name: String::from_utf8_lossy(&name_vec).into_owned(),
            version: String::from_utf8_lossy(&version_vec).into_owned(),
            chain_id: self.network.chain_id(),
            verifying_contract: Address::from_slice(&self.bridge_contract.as_bytes()),
        )
    }
}

#[derive(Debug)]
pub enum ECDSAVerificationError {
    InvalidSignature,
    InvalidValueForV,
    InvalidValueForRS,
    InvalidMessageFormat,
    BadSignature,
    FailedToHashStringData,
    FailedToHash32BytesHexData,
}

pub enum BridgeContractMethod {
    ReferenceRateUpdatedAt,
    CheckReferenceRate,
    UpdateReferenceRate,
    PublishRoot,
    TriggerGrowth,
    AddAuthor,
    RemoveAuthor,
}

impl BridgeContractMethod {
    pub fn name_as_bytes(&self) -> &[u8] {
        match self {
            BridgeContractMethod::ReferenceRateUpdatedAt => b"referenceRateUpdatedAt",
            BridgeContractMethod::CheckReferenceRate => b"checkReferenceRate",
            BridgeContractMethod::UpdateReferenceRate => b"updateReferenceRate",
            BridgeContractMethod::PublishRoot => b"publishRoot",
            BridgeContractMethod::TriggerGrowth => b"triggerGrowth",
            BridgeContractMethod::AddAuthor => b"addAuthor",
            BridgeContractMethod::RemoveAuthor => b"removeAuthor",
        }
    }
}

impl TryFrom<&[u8]> for BridgeContractMethod {
    type Error = ();

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        match value {
            b"referenceRateUpdatedAt" => Ok(BridgeContractMethod::ReferenceRateUpdatedAt),
            b"checkReferenceRate" => Ok(BridgeContractMethod::CheckReferenceRate),
            b"updateReferenceRate" => Ok(BridgeContractMethod::UpdateReferenceRate),
            b"publishRoot" => Ok(BridgeContractMethod::PublishRoot),
            b"triggerGrowth" => Ok(BridgeContractMethod::TriggerGrowth),
            b"addAuthor" => Ok(BridgeContractMethod::AddAuthor),
            b"removeAuthor" => Ok(BridgeContractMethod::RemoveAuthor),
            _ => Err(()),
        }
    }
}

sol! {
    struct PublishRoot {
        bytes32 root;
        uint64 expiry;
        uint32 tx_id;
    }
}

sol! {
    struct TriggerGrowth {
        uint128 rewards;
        uint128 avg_staked;
        uint32 period;
        uint64 expiry;
        uint32 tx_id;
    }
}

sol! {
    struct AddAuthor {
        bytes calldata t1_pub_key;
        bytes32 t2_pub_key;
        uint64 expiry;
        uint32 tx_id;
    }
}

sol! {
    struct RemoveAuthor {
        bytes32 t2_pub_key;
        bytes calldata t1_pub_key;
        uint64 expiry;
        uint32 tx_id;
    }
}

sol! {
    struct LowerData {
        address token;
        uint256 amount;
        address recipient;
        uint32 lowerId;
    }
}

impl TryFrom<LowerParams> for LowerData {
    type Error = ();

    fn try_from(lower_params: LowerParams) -> Result<Self, Self::Error> {
        if lower_params.len() != PACKED_LOWER_PARAM_SIZE {
            return Err(())
        }

        let token = Address::from_slice(&lower_params[0..20]);
        let amount = AlloyU256::try_from_be_slice(&lower_params[36..52]).ok_or(())?;
        let recipient = Address::from_slice(&lower_params[52..72]);
        let lower_id = u32::from_be_bytes(lower_params[72..76].try_into().map_err(|_| ())?);

        Ok(LowerData { token, amount, recipient, lowerId: lower_id })
    }
}

pub fn create_lower_proof_hash(
    lower_params: LowerParams,
    domain: Eip712Domain,
) -> Result<H256, ()> {
    let claim_lower = LowerData::try_from(lower_params)?;
    let hash = eip712_hash(&claim_lower, &domain);

    Ok(hash)
}

pub fn eip712_hash<T: SolStruct>(data: &T, domain: &Eip712Domain) -> H256 {
    let hash: AlloyB256 = data.eip712_signing_hash(domain);
    H256::from_slice(&hash.0)
}

fn parse_from_utf8<T: std::str::FromStr>(bytes: &[u8]) -> Result<T, ()> {
    str::from_utf8(bytes).map_err(|_| ())?.parse::<T>().map_err(|_| ())
}

pub fn create_function_confirmation_hash(
    function_name: Vec<u8>,
    params: Vec<(Vec<u8>, Vec<u8>)>,
    domain: Eip712Domain,
) -> Result<H256, ()> {
    let extract_tx_id_and_expiry = |params: &Vec<(Vec<u8>, Vec<u8>)>| -> Result<(u32, u64), ()> {
        if params.len() < 2 {
            return Err(()) // Ensure there are at least 2 elements in params
        }
        let last_two_elements = &params[params.len() - 2..];
        let tx_id = parse_from_utf8::<u32>(&last_two_elements[0].1)?;
        let expiry = parse_from_utf8::<u64>(&last_two_elements[1].1)?;

        Ok((tx_id, expiry))
    };

    match BridgeContractMethod::try_from(&function_name[..])? {
        BridgeContractMethod::PublishRoot => {
            if params.len() != 3 {
                return Err(()) // Ensure there are at least 3 elements in params
            }
            let root_data = H256::from_slice(&params[0].1);
            let (tx_id, expiry) = extract_tx_id_and_expiry(&params)?;
            let data = PublishRoot {
                root: FixedBytes::from_slice(root_data.as_fixed_bytes()),
                expiry,
                tx_id,
            };
            return Ok(eip712_hash(&data, &domain))
        },
        BridgeContractMethod::TriggerGrowth => {
            if params.len() != 5 {
                return Err(()) // Ensure there are at least 5 elements in params
            }

            let rewards = parse_from_utf8::<u128>(&params[0].1)?;
            let avg_staked = parse_from_utf8::<u128>(&params[1].1)?;
            let period = parse_from_utf8::<u32>(&params[2].1)?;

            let (tx_id, expiry) = extract_tx_id_and_expiry(&params)?;
            let data = TriggerGrowth { rewards, avg_staked, period, expiry, tx_id };
            return Ok(eip712_hash(&data, &domain))
        },

        BridgeContractMethod::AddAuthor => {
            if params.len() != 4 {
                return Err(())
            }

            let t1_pub_key: Bytes = Bytes::from(params[0].1.clone());
            let t2_pub_key: AlloyB256 = AlloyB256::from_slice(&params[1].1);

            let (tx_id, expiry) = extract_tx_id_and_expiry(&params)?;
            let data = AddAuthor { t1_pub_key, t2_pub_key, expiry, tx_id };
            return Ok(eip712_hash(&data, &domain))
        },

        BridgeContractMethod::RemoveAuthor => {
            if params.len() != 4 {
                return Err(())
            }

            let t2_pub_key: AlloyB256 = AlloyB256::from_slice(&params[0].1);
            let t1_pub_key: Bytes = Bytes::from(params[1].1.clone());

            let (tx_id, expiry) = extract_tx_id_and_expiry(&params)?;
            let data = RemoveAuthor { t2_pub_key, t1_pub_key, expiry, tx_id };
            return Ok(eip712_hash(&data, &domain))
        },

        _ => return Err(()),
    }
}

#[test]
fn test_conversion_endianess() {
    let value_in_u32: u32 = 123456;
    let value_in_u64: u64 = 123456;
    let value_in_u128: u128 = 123456;
    let params = vec![
        (b"uint32".to_vec(), format!("{}", value_in_u32).as_bytes().to_vec()),
        (b"uint64".to_vec(), format!("{}", value_in_u64).as_bytes().to_vec()),
        (b"uint128".to_vec(), format!("{}", value_in_u128).as_bytes().to_vec()),
    ];

    let restored_u32 = parse_from_utf8::<u32>(&params[0].1).expect("msg should be valid");
    let restored_u64 = parse_from_utf8::<u64>(&params[1].1).expect("msg should be valid");
    let restored_u128 = parse_from_utf8::<u128>(&params[2].1).expect("msg should be valid");

    assert!(restored_u32 == value_in_u32, "Restored value does not match original");
    assert!(restored_u64 == value_in_u64, "Restored value does not match original");
    assert!(restored_u128 == value_in_u128, "Restored value does not match original");
}
