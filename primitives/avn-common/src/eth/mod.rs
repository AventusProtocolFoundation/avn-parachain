#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::string::String;
#[cfg(not(feature = "std"))]
use alloc::{string::String, vec::Vec};

use codec::{Decode, Encode, MaxEncodedLen};
use core::str;
use sp_core::{ConstU32, H160};
use sp_io::hashing::blake2_256;
use sp_runtime::{scale_info::TypeInfo, BoundedVec};
use sp_std::vec::Vec;
use alloy_primitives::Address;
use alloy_sol_types::{eip712_domain, Eip712Domain};

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
