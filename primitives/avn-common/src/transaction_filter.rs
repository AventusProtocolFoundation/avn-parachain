use async_trait::async_trait;
use codec::Encode;
use futures::Stream;
use sc_transaction_pool_api::{
    error::Error as PoolError, ChainEvent, ImportNotificationStream, MaintainedTransactionPool,
    PoolFuture, PoolStatus, ReadyTransactions, TransactionFor, TransactionPool, TransactionSource,
    TxHash,
};
use sp_runtime::traits::{Block as BlockT, NumberFor};
use std::{collections::HashMap, future::Future, marker::PhantomData, pin::Pin, sync::Arc};

pub trait ExtrinsicFilter: Send + Sync + 'static {
    fn is_banned(&self, xt: &sp_core::Bytes) -> bool;
}

pub struct FilteredPool<Pool, Filter> {
    inner: Arc<Pool>,
    filter: Arc<Filter>,
}

impl<Pool, Filter> FilteredPool<Pool, Filter>
where
    Filter: ExtrinsicFilter,
{
    pub fn new(inner: Arc<Pool>, filter: Arc<Filter>) -> Self {
        Self { inner, filter }
    }

    fn check_banned(&self, xt: &impl Encode) -> Result<(), PoolError> {
        if self.filter.is_banned(&xt.encode().into()) {
            log::debug!(target: "tx-filter", "Transaction rejected by filter");
            return Err(PoolError::InvalidTransaction(
                sp_runtime::transaction_validity::InvalidTransaction::Call,
            ))
        }
        Ok(())
    }
}

impl<Pool, Filter> Clone for FilteredPool<Pool, Filter> {
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone(), filter: self.filter.clone() }
    }
}

impl<Pool, Filter> TransactionPool for FilteredPool<Pool, Filter>
where
    Pool: TransactionPool,
    Filter: ExtrinsicFilter,
    Pool::Error: 'static,
{
    type Block = Pool::Block;
    type Hash = Pool::Hash;
    type InPoolTransaction = Pool::InPoolTransaction;
    type Error = Pool::Error;

    fn submit_at(
        &self,
        at: <Self::Block as BlockT>::Hash,
        source: TransactionSource,
        xts: Vec<TransactionFor<Self>>,
    ) -> PoolFuture<Vec<Result<TxHash<Self>, Self::Error>>, Self::Error> {
        let mut allowed_xts = Vec::with_capacity(xts.len());
        let mut rejections = Vec::new();

        for xt in xts {
            match self.check_banned(&xt) {
                Ok(_) => allowed_xts.push(xt),
                Err(e) => rejections.push(Err(e.into())),
            }
        }

        if allowed_xts.is_empty() {
            return Box::pin(async move { Ok(rejections) })
        }

        let inner_future = self.inner.submit_at(at, source, allowed_xts);

        Box::pin(async move {
            let mut inner_results = inner_future.await?;
            rejections.append(&mut inner_results);
            Ok(rejections)
        })
    }

    fn submit_one(
        &self,
        at: <Self::Block as BlockT>::Hash,
        source: TransactionSource,
        xt: TransactionFor<Self>,
    ) -> PoolFuture<TxHash<Self>, Self::Error> {
        if let Err(e) = self.check_banned(&xt) {
            return Box::pin(async move { Err(e.into()) })
        }
        self.inner.submit_one(at, source, xt)
    }

    fn submit_and_watch(
        &self,
        at: <Self::Block as BlockT>::Hash,
        source: TransactionSource,
        xt: TransactionFor<Self>,
    ) -> PoolFuture<
        Pin<
            Box<
                dyn Stream<
                        Item = sc_transaction_pool_api::TransactionStatus<
                            TxHash<Self>,
                            <Self::Block as BlockT>::Hash,
                        >,
                    > + Send,
            >,
        >,
        Self::Error,
    > {
        if let Err(e) = self.check_banned(&xt) {
            return Box::pin(async move { Err(e.into()) })
        }
        self.inner.submit_and_watch(at, source, xt)
    }

    fn ready_at(
        &self,
        at: NumberFor<Self::Block>,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Box<dyn ReadyTransactions<Item = Arc<Self::InPoolTransaction>> + Send>,
                > + Send,
        >,
    > {
        self.inner.ready_at(at)
    }

    fn ready(&self) -> Box<dyn ReadyTransactions<Item = Arc<Self::InPoolTransaction>> + Send> {
        self.inner.ready()
    }

    fn remove_invalid(&self, hashes: &[TxHash<Self>]) -> Vec<Arc<Self::InPoolTransaction>> {
        self.inner.remove_invalid(hashes)
    }

    fn status(&self) -> PoolStatus {
        self.inner.status()
    }

    fn import_notification_stream(&self) -> ImportNotificationStream<TxHash<Self>> {
        self.inner.import_notification_stream()
    }

    fn on_broadcasted(&self, propagations: HashMap<TxHash<Self>, Vec<String>>) {
        self.inner.on_broadcasted(propagations)
    }

    fn hash_of(&self, xt: &TransactionFor<Self>) -> TxHash<Self> {
        self.inner.hash_of(xt)
    }

    fn ready_transaction(&self, hash: &TxHash<Self>) -> Option<Arc<Self::InPoolTransaction>> {
        self.inner.ready_transaction(hash)
    }

    fn futures(&self) -> Vec<Self::InPoolTransaction> {
        self.inner.futures()
    }
}

#[async_trait]
impl<Pool, Filter> MaintainedTransactionPool for FilteredPool<Pool, Filter>
where
    Pool: MaintainedTransactionPool,
    Filter: ExtrinsicFilter,
    Pool::Error: 'static,
{
    async fn maintain(&self, event: ChainEvent<Self::Block>) {
        self.inner.maintain(event).await
    }
}

impl<Pool, Filter> sc_transaction_pool_api::LocalTransactionPool for FilteredPool<Pool, Filter>
where
    Pool: sc_transaction_pool_api::LocalTransactionPool,
    Filter: ExtrinsicFilter,
{
    type Block = Pool::Block;
    type Hash = Pool::Hash;
    type Error = Pool::Error;

    fn submit_local(
        &self,
        at: <Self::Block as BlockT>::Hash,
        xt: sc_transaction_pool_api::LocalTransactionFor<Self>,
    ) -> Result<Self::Hash, Self::Error> {
        if let Err(e) = self.check_banned(&xt) {
            return Err(e.into())
        }
        self.inner.submit_local(at, xt)
    }
}

/// A generic filter implementation that uses a decoder and a predicate.
pub struct DecodingFilter<Call, Decoder, Predicate> {
    /// Function to decode raw bytes into a Call
    decoder: Decoder,
    /// Function to check if a Call is allowed
    predicate: Predicate,
    /// Whether the filter is active
    enabled: bool,
    /// Whether to log rejections
    log_rejections: bool,
    _phantom: PhantomData<Call>,
}

impl<Call, Decoder, Predicate> DecodingFilter<Call, Decoder, Predicate>
where
    Decoder: Fn(&[u8]) -> Result<Call, codec::Error> + Send + Sync + 'static,
    Predicate: Fn(&Call) -> bool + Send + Sync + 'static,
    Call: Send + Sync + 'static,
{
    pub fn new(
        enabled: bool,
        log_rejections: bool,
        decoder: Decoder,
        predicate: Predicate,
    ) -> Self {
        Self { enabled, log_rejections, decoder, predicate, _phantom: PhantomData }
    }
}

impl<Call, Decoder, Predicate> ExtrinsicFilter for DecodingFilter<Call, Decoder, Predicate>
where
    Decoder: Fn(&[u8]) -> Result<Call, codec::Error> + Send + Sync + 'static,
    Predicate: Fn(&Call) -> bool + Send + Sync + 'static,
    Call: Send + Sync + 'static,
{
    fn is_banned(&self, xt: &sp_core::Bytes) -> bool {
        if !self.enabled {
            return false
        }

        match (self.decoder)(xt) {
            Ok(call) => {
                let allowed = (self.predicate)(&call);
                if !allowed && self.log_rejections {
                    log::warn!(target: "tx-filter", "Rejected disallowed transaction");
                }
                !allowed // Banned if not allowed
            },
            Err(e) => {
                if self.log_rejections {
                    log::warn!(target: "tx-filter", "Rejected malformed transaction: {:?}", e);
                }
                true // Banned (Fail Secure)
            },
        }
    }
}
