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

impl SendRequest {
    pub fn extend_params<T: Config>(
        &self,
        expiry: u64,
    ) -> Result<BoundedVec<(BoundedVec<u8, TypeLimit>, BoundedVec<u8, ValueLimit>), ParamsLimit>, Error<T>> {
        let mut extended_params = util::unbound_params(&self.params);
        extended_params.push((eth::UINT256.to_vec(), expiry.to_string().into_bytes()));
        extended_params.push((eth::UINT32.to_vec(), self.id.to_string().into_bytes()));

        Ok(util::bound_params(&extended_params)?)
    }

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

#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen)]
pub struct ActiveTransactionData<T: Config> {
    pub request: SendRequest,
    pub confirmation_data: ConfirmationData,
    pub data: ActiveEthTransactionData<T>,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
pub struct ActiveEthTransactionData<T: Config> {
    pub function_name: BoundedVec<u8, FunctionLimit>,
    pub eth_tx_params: BoundedVec<(BoundedVec<u8, TypeLimit>, BoundedVec<u8, ValueLimit>), ParamsLimit>,
    pub sender: T::AccountId,
    pub expiry: u64,
    pub eth_tx_hash: H256,
    pub success_corroborations: BoundedVec<T::AccountId, ConfirmationsLimit>,
    pub failure_corroborations: BoundedVec<T::AccountId, ConfirmationsLimit>,
    pub valid_tx_hash_corroborations: BoundedVec<T::AccountId, ConfirmationsLimit>,
    pub invalid_tx_hash_corroborations: BoundedVec<T::AccountId, ConfirmationsLimit>,
    pub tx_succeeded: bool,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
pub struct ActiveRequest<T: Config> {
    pub request: Request,
    pub confirmation_data: ConfirmationData,
    pub tx_data: Option<ActiveEthTransactionData<T>>,
    pub last_updated: T::BlockNumber,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
pub struct ConfirmationData {
    pub msg_hash: H256,
    pub confirmations: BoundedVec<ecdsa::Signature, ConfirmationsLimit>,
}

impl<T: Config> Identifiable for ActiveRequest<T> {
    fn id(&self) -> EthereumId {
        return self.request.id();
    }
}

impl<T: Config> ActiveRequest<T> {
    pub fn as_active_tx(&self) -> Result<ActiveTransactionData<T>, Error<T>> {
        if self.tx_data.is_none() {
            return Err(Error::<T>::InvalidSendRequest)
        }

        match self.request {
            Request::Send(ref req) =>
                return Ok(ActiveTransactionData {
                    request: req.clone(),
                    confirmation_data: self.confirmation_data.clone(),
                    data: self.tx_data.as_ref().expect("data is not null").clone(),
                }),
            _ => return Err(Error::<T>::InvalidSendRequest),
        }


    }
}

impl<T: Config> Identifiable for ActiveTransactionData<T> {
    fn id(&self) -> EthereumId {
        return self.request.id();
    }
}

