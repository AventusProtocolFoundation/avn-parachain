use crate::*;

pub trait Identifiable {
    fn id(&self) -> EthereumId;
}

// The different types of request this pallet can handle.
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen)]
pub enum Request {
    Send(SendRequestData),
    LowerProof(LowerProofRequestData),
}

impl Default for Request {
    fn default() -> Self {
        Request::Send(Default::default())
    }
}

impl Identifiable for Request {
    fn id(&self) -> EthereumId {
        match self {
            Request::Send(req) => req.id(),
            Request::LowerProof(req) => req.id(),
        }
    }
}

// Request data for a transaction we are sending to Ethereum
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
pub struct SendRequestData {
    pub id: EthereumId,
    pub function_name: BoundedVec<u8, FunctionLimit>,
    pub params: BoundedVec<(BoundedVec<u8, TypeLimit>, BoundedVec<u8, ValueLimit>), ParamsLimit>,
}

impl Identifiable for SendRequestData {
    fn id(&self) -> EthereumId {
        return self.id
    }
}

// Request data for a message that requires confirmation for Ethereum
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
pub struct LowerProofRequestData {
    pub id: EthereumId,
    pub params: BoundedVec<(BoundedVec<u8, TypeLimit>, BoundedVec<u8, ValueLimit>), ParamsLimit>,
}

impl Identifiable for LowerProofRequestData {
    fn id(&self) -> EthereumId {
        return self.id
    }
}
// Persisten storage struct to hold transactions sent to Ethereum
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
pub struct TransactionData<T: Config> {
    pub function_name: BoundedVec<u8, FunctionLimit>,
    pub params:
        BoundedVec<(BoundedVec<u8, TypeLimit>, BoundedVec<u8, ValueLimit>), ParamsLimit>,
    pub sender: T::AccountId,
    pub eth_tx_hash: H256,
    pub tx_succeeded: bool,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
pub struct ActiveTransactionData<T: Config> {
    pub id: EthereumId,
    pub request_data: SendRequestData,
    pub data: TransactionData<T>,
    pub expiry: u64,
    pub msg_hash: H256,
    pub last_updated: T::BlockNumber,
    pub confirmations: BoundedVec<ecdsa::Signature, ConfirmationsLimit>,
    pub success_corroborations: BoundedVec<T::AccountId, ConfirmationsLimit>,
    pub failure_corroborations: BoundedVec<T::AccountId, ConfirmationsLimit>,
    pub valid_tx_hash_corroborations: BoundedVec<T::AccountId, ConfirmationsLimit>,
    pub invalid_tx_hash_corroborations: BoundedVec<T::AccountId, ConfirmationsLimit>,
}
