use crate::*;

pub trait Identifiable {
    fn id(&self) -> EthereumId;
}

// The different types of request this pallet can handle.
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen)]
pub enum Request {
    Send(SendRequestData),
    Proof(ProofRequestData), /*This of a better name*/
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
            Request::Proof(req) => req.id(),
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

impl SendRequestData {
    pub fn extend_params<T: Config>(
        &self,
        expiry: u64,
    ) -> Result<
        BoundedVec<(BoundedVec<u8, TypeLimit>, BoundedVec<u8, ValueLimit>), ParamsLimit>,
        Error<T>,
    > {
        let mut extended_params = util::unbound_params(&self.params);
        extended_params.push((eth::UINT256.to_vec(), expiry.to_string().into_bytes()));
        extended_params.push((eth::UINT32.to_vec(), self.id.to_string().into_bytes()));

        Ok(util::bound_params(&extended_params)?)
    }
}

impl Identifiable for SendRequestData {
    fn id(&self) -> EthereumId {
        return self.id
    }
}

// Request data for a message that requires confirmation for Ethereum
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
pub struct ProofRequestData {
    pub id: EthereumId,
    pub params: BoundedVec<(BoundedVec<u8, TypeLimit>, BoundedVec<u8, ValueLimit>), ParamsLimit>,
}

impl Identifiable for ProofRequestData {
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

// Persisten storage struct to hold transactions sent to Ethereum
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
pub struct TransactionData<T: Config> {
    pub function_name: BoundedVec<u8, FunctionLimit>,
    pub params: BoundedVec<(BoundedVec<u8, TypeLimit>, BoundedVec<u8, ValueLimit>), ParamsLimit>,
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
    // Function to convert an active request into an active transaction request
    pub fn as_active_tx(&self) -> Result<ActiveTransactionData<T>, Error<T>> {
        if self.tx_data.is_none() {
            return Err(Error::<T>::InvalidSendRequest)
        }

        match self.request {
            Request::Send(ref req) =>
                return Ok(ActiveTransactionData {
                    request: req.clone(),
                    confirmation: self.confirmation.clone(),
                    data: self.tx_data.as_ref().expect("data is not null").clone(),
                }),
            _ => return Err(Error::<T>::InvalidSendRequest),
        }
    }
}

// Active request data specific for a transaction. 'data' is not optional.
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen)]
pub struct ActiveTransactionData<T: Config> {
    pub request: SendRequestData,
    pub confirmation: ActiveConfirmation,
    pub data: ActiveEthTransaction<T>,
}

impl<T: Config> Identifiable for ActiveTransactionData<T> {
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