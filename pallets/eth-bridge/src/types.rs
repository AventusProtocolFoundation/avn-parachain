use crate::*;

pub trait Identifiable {
    fn id(&self) -> EthereumId;
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen)]
pub enum Request {
    Send(SendRequest),
    Confirm(ConfirmationRequest),
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
            Request::Confirm(req) => req.id(),
        }
    }
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
pub struct SendRequest {
    pub id: EthereumId,
    pub function_name: BoundedVec<u8, FunctionLimit>,
    pub params: BoundedVec<(BoundedVec<u8, TypeLimit>, BoundedVec<u8, ValueLimit>), ParamsLimit>,
}

impl Identifiable for SendRequest {
    fn id(&self) -> EthereumId {
        return self.id;
    }
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
pub struct ConfirmationRequest {
    pub id: EthereumId,
    pub params: BoundedVec<(BoundedVec<u8, TypeLimit>, BoundedVec<u8, ValueLimit>), ParamsLimit>,
}

impl Identifiable for ConfirmationRequest {
    fn id(&self) -> EthereumId {
        return self.id;
    }
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
pub struct ActiveConfirmationData<T: Config> {
    pub request: Request,
    pub msg_hash: H256,
    pub sender: Option<T::AccountId>,
    pub last_updated: T::BlockNumber,
    pub confirmations: BoundedVec<ecdsa::Signature, ConfirmationsLimit>,
}

impl<T: Config> Identifiable for ActiveConfirmationData<T> {
    fn id(&self) -> EthereumId {
        return self.request.id();
    }
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
pub struct TransactionData<T: Config> {
    pub function_name: BoundedVec<u8, FunctionLimit>,
    pub params: BoundedVec<(BoundedVec<u8, TypeLimit>, BoundedVec<u8, ValueLimit>), ParamsLimit>,
    pub sender: T::AccountId,
    pub eth_tx_hash: H256,
    pub tx_succeeded: bool,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
pub struct ActiveTransactionData<T: Config> {
    pub request: Request,
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

impl<T: Config> Identifiable for ActiveTransactionData<T> {
    fn id(&self) -> EthereumId {
        return self.request.id();
    }
}

// #[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
// pub struct SendRequest {
//     pub tx_id: EthereumId,
//     pub function_name: BoundedVec<u8, FunctionLimit>,
//     pub params:
//         BoundedVec<(BoundedVec<u8, TypeLimit>, BoundedVec<u8, ValueLimit>), ParamsLimit>,
//     pub request_type: RequestType,
// }

// #[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
// pub struct ConfirmationRequest {
//     pub tx_id: EthereumId,
//     pub params:
//         BoundedVec<(BoundedVec<u8, TypeLimit>, BoundedVec<u8, ValueLimit>), ParamsLimit>,
// }

// #[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
// pub struct TransactionData<T: Config> {
//     pub function_name: BoundedVec<u8, FunctionLimit>,
//     pub params:
//         BoundedVec<(BoundedVec<u8, TypeLimit>, BoundedVec<u8, ValueLimit>), ParamsLimit>,
//     pub sender: T::AccountId,
//     pub eth_tx_hash: H256,
//     pub tx_succeeded: bool,
// }

// #[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
// pub struct ActiveTransactionData<T: Config> {
//     pub id: EthereumId,
//     pub request_data: Request,
//     pub data: TransactionData<T>,
//     pub expiry: u64,
//     pub msg_hash: H256,
//     pub last_updated: T::BlockNumber,
//     pub confirmations: BoundedVec<ecdsa::Signature, ConfirmationsLimit>,
//     pub success_corroborations: BoundedVec<T::AccountId, ConfirmationsLimit>,
//     pub failure_corroborations: BoundedVec<T::AccountId, ConfirmationsLimit>,
//     pub valid_tx_hash_corroborations: BoundedVec<T::AccountId, ConfirmationsLimit>,
//     pub invalid_tx_hash_corroborations: BoundedVec<T::AccountId, ConfirmationsLimit>,
// }

// #[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
// pub struct ActiveTransactionData2<T: Config> {
//     pub id: EthereumId,
//     pub data: TransactionData<T>,
//     pub expiry: u64,
//     pub confirmation: ActiveConfirmationData<T>,
//     pub success_corroborations: BoundedVec<T::AccountId, ConfirmationsLimit>,
//     pub failure_corroborations: BoundedVec<T::AccountId, ConfirmationsLimit>,
//     pub valid_tx_hash_corroborations: BoundedVec<T::AccountId, ConfirmationsLimit>,
//     pub invalid_tx_hash_corroborations: BoundedVec<T::AccountId, ConfirmationsLimit>,
// }
