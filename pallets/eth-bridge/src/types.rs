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
    pub lower_id: u32,
    pub params: BoundedVec<(BoundedVec<u8, TypeLimit>, BoundedVec<u8, ValueLimit>), ParamsLimit>,
}

impl Identifiable for LowerProofRequestData {
    fn id(&self) -> EthereumId {
        return self.id
    }
}

// Data related to generating confirmations
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
pub struct ActiveConfirmation {
    pub msg_hash: H256,
    pub confirmations: BoundedVec<ecdsa::Signature, ConfirmationsLimit>,
}

// Persistent storage struct to hold lower proof that can be claimed on Ethereum
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
pub struct LowerProofData {
    pub params: BoundedVec<(BoundedVec<u8, TypeLimit>, BoundedVec<u8, ValueLimit>), ParamsLimit>,
    pub abi_encoded_lower_data: BoundedVec<u8, LowerDataLimit>,
}

// Persistent storage struct to hold transactions sent to Ethereum
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
pub struct TransactionData<T: Config> {
    pub function_name: BoundedVec<u8, FunctionLimit>,
    pub params:
        BoundedVec<(BoundedVec<u8, TypeLimit>, BoundedVec<u8, ValueLimit>), ParamsLimit>,
    pub sender: T::AccountId,
    pub eth_tx_hash: H256,
    pub tx_succeeded: bool,
}

// Storage item for the active request being processed
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
pub struct ActiveRequestData<T: Config> {
    pub request: Request,
    pub confirmation: ActiveConfirmation,
    pub tx_data: Option<ActiveEthTransaction<T>>,
    pub last_updated: T::BlockNumber,
}

impl<T: Config> Identifiable for ActiveRequestData<T> {
    fn id(&self) -> EthereumId {
        return self.request.id()
    }
}

impl<T: Config> ActiveRequestData<T> {
    // Function to convert an active request into an active transaction request.
    pub fn as_active_tx(self) -> Result<ActiveTransactionDataV2<T>, Error<T>> {
        if self.tx_data.is_none() {
            return Err(Error::<T>::InvalidSendRequest)
        }

        match self.request {
            Request::Send(req) => {
                let tx_data = self.tx_data.expect("data is not null");
                Ok(ActiveTransactionDataV2 {
                    request: req,
                    confirmation: self.confirmation,
                    data: tx_data,
                })
            }
            _ => return Err(Error::<T>::InvalidSendRequest),
        }
    }
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

// Active request data specific for a transaction. 'data' is not optional.
// ** NOTE: ** Next PR will rename this to ActiveTransactionData and remove the existing ActiveTransactionData struct above
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen)]
pub struct ActiveTransactionDataV2<T: Config> {
    pub request: SendRequestData,
    pub confirmation: ActiveConfirmation,
    pub data: ActiveEthTransaction<T>,
}

impl<T: Config> Identifiable for ActiveTransactionDataV2<T> {
    fn id(&self) -> EthereumId {
        return self.request.id()
    }
}

// Transient data used for an active send transaction request
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
pub struct ActiveEthTransaction<T: Config> {
    pub function_name: BoundedVec<u8, FunctionLimit>,
    pub eth_tx_params:
        BoundedVec<(BoundedVec<u8, TypeLimit>, BoundedVec<u8, ValueLimit>), ParamsLimit>,
    pub sender: T::AccountId,
    pub expiry: u64,
    pub eth_tx_hash: H256,
    pub success_corroborations: BoundedVec<T::AccountId, ConfirmationsLimit>,
    pub failure_corroborations: BoundedVec<T::AccountId, ConfirmationsLimit>,
    pub valid_tx_hash_corroborations: BoundedVec<T::AccountId, ConfirmationsLimit>,
    pub invalid_tx_hash_corroborations: BoundedVec<T::AccountId, ConfirmationsLimit>,
    pub tx_succeeded: bool,
}
