use alloy_sol_types::Eip712Domain;
use codec::{Decode, Encode, MaxEncodedLen};
use sp_core::{hashing::blake2_256, ConstU32, H160, U256};
use sp_runtime::{scale_info::TypeInfo, BoundedVec};

use alloy_primitives::{Address, FixedBytes, U256 as AlloyU256};

#[derive(Encode, Decode, Default, Clone, Debug, PartialEq, Eq, TypeInfo, MaxEncodedLen)]
pub enum EthereumNetwork {
    #[default]
    Ethereum,
    EWC,
    Sepolia,
    Holesky,
    Volta,
    Custom(U256),
}

impl EthereumNetwork {
    pub fn chain_id(&self) -> U256 {
        match self {
            EthereumNetwork::Ethereum => U256::from(1),
            EthereumNetwork::EWC => U256::from(246),
            EthereumNetwork::Sepolia => U256::from(11155111),
            EthereumNetwork::Holesky => U256::from(17000),
            EthereumNetwork::Volta => U256::from(73799),
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
        let mut buffer = [0u8; 32];
        self.network.chain_id().to_little_endian(&mut buffer);
        let name_vec: Vec<u8> = self.name.into_inner();
        let version_vec: Vec<u8> = self.version.into_inner();

        Eip712Domain {
            name: Some(String::from_utf8_lossy(&name_vec).into_owned().into()),
            version: Some(String::from_utf8_lossy(&version_vec).into_owned().into()),
            chain_id: Some(AlloyU256::from_le_bytes(buffer)),
            verifying_contract: Some(Address::from_slice(&self.bridge_contract.as_bytes())),
            salt: self.salt.map(|s| FixedBytes::from(&s)),
        }
    }
}
