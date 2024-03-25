use crate::*;
use sp_avn_common::event_types::ValidEvents;
type EventsTypesLimit = ConstU32<20>;
type EthBridgeEventsFilter = BoundedBTreeSet<ValidEvents, EventsTypesLimit>;

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

impl Request {
    pub fn id_matches(&self, id: &u32) -> bool {
        match self {
            Request::Send(req) => &req.tx_id == id,
            Request::LowerProof(req) => &req.lower_id == id,
        }
    }
}

// Request data for a transaction we are sending to Ethereum
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
pub struct SendRequestData {
    pub tx_id: EthereumId,
    pub function_name: BoundedVec<u8, FunctionLimit>,
    pub params: BoundedVec<(BoundedVec<u8, TypeLimit>, BoundedVec<u8, ValueLimit>), ParamsLimit>,
    pub caller_id: BoundedVec<u8, CallerIdLimit>,
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
        extended_params.push((eth::UINT32.to_vec(), self.tx_id.to_string().into_bytes()));

        Ok(util::bound_params(&extended_params)?)
    }
}

// Request data for a message that requires confirmation for Ethereum
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen)]
pub struct LowerProofRequestData {
    pub lower_id: LowerId,
    pub params: LowerParams,
    pub caller_id: BoundedVec<u8, CallerIdLimit>,
}

// Data related to generating confirmations
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
pub struct ActiveConfirmation {
    pub msg_hash: H256,
    pub confirmations: BoundedVec<ecdsa::Signature, ConfirmationsLimit>,
}

// Persistent storage struct to hold transactions sent to Ethereum
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
    pub last_updated: BlockNumberFor<T>,
}

impl<T: Config> ActiveRequestData<T> {
    // Function to convert an active request into an active transaction request.
    pub fn as_active_tx(self) -> Result<ActiveTransactionData<T>, Error<T>> {
        if self.tx_data.is_none() {
            return Err(Error::<T>::InvalidSendRequest)
        }

        match self.request {
            Request::Send(req) => {
                let tx_data = self.tx_data.expect("data is not null");
                Ok(ActiveTransactionData {
                    request: req,
                    confirmation: self.confirmation,
                    data: tx_data,
                })
            },
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

#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, Default, TypeInfo, MaxEncodedLen)]
pub struct ActiveEthRange {
    pub range: EthBlockRange,
    pub partition: u16,
    pub event_types_filter: EthBridgeEventsFilter,
}

impl ActiveEthRange {
    pub fn is_initial_range(&self) -> bool {
        *self == Default::default()
    }
}
