//! Transaction pool filtering for public RPC nodes.
//!
//! This module provides infrastructure for filtering extrinsics before they enter
//! the transaction pool. This is useful for public RPC nodes that want to restrict
//! which types of transactions they accept.

use async_trait::async_trait;
use codec::Encode;
use sc_transaction_pool_api::{
    error::Error as PoolError, ChainEvent, ImportNotificationStream, MaintainedTransactionPool,
    PoolFuture, PoolStatus, ReadyTransactions, TransactionFor, TransactionPool, TransactionSource,
    TxHash,
};
use sp_runtime::{
    generic::BlockId,
    traits::{Block as BlockT, NumberFor},
};
use std::{collections::HashMap, marker::PhantomData, pin::Pin, sync::Arc};

/// Trait for filtering extrinsics before they enter the transaction pool.
///
/// Implementations can inspect the raw encoded extrinsic bytes and decide
/// whether to accept or reject the transaction.
pub trait ExtrinsicFilter: Send + Sync + 'static {
    /// Returns `true` if the extrinsic should be rejected (banned).
    fn is_banned(&self, xt: &sp_core::Bytes) -> bool;
}

/// A transaction pool wrapper that filters extrinsics before submission.
///
/// This wraps an inner pool and applies an `ExtrinsicFilter` to all submission
/// operations. Read operations (ready, status, etc.) delegate directly to the
/// inner pool without filtering.
pub struct FilteredPool<Pool> {
    inner: Arc<Pool>,
    filter: Arc<dyn ExtrinsicFilter>,
}

impl<Pool> FilteredPool<Pool> {
    /// Creates a new filtered pool wrapping the given inner pool.
    pub fn new(inner: Arc<Pool>, filter: Arc<dyn ExtrinsicFilter>) -> Self {
        Self { inner, filter }
    }

    fn check_banned(&self, xt: &impl Encode) -> Result<(), PoolError> {
        if self.filter.is_banned(&xt.encode().into()) {
            return Err(PoolError::InvalidTransaction(
                sp_runtime::transaction_validity::InvalidTransaction::Call,
            ))
        }
        Ok(())
    }
}

impl<Pool> Clone for FilteredPool<Pool> {
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone(), filter: self.filter.clone() }
    }
}

impl<Pool> TransactionPool for FilteredPool<Pool>
where
    Pool: TransactionPool,
    Pool::Error: 'static,
{
    type Block = Pool::Block;
    type Hash = Pool::Hash;
    type InPoolTransaction = Pool::InPoolTransaction;
    type Error = Pool::Error;

    fn submit_at(
        &self,
        at: &BlockId<Self::Block>,
        source: TransactionSource,
        xts: Vec<TransactionFor<Self>>,
    ) -> PoolFuture<Vec<Result<TxHash<Self>, Self::Error>>, Self::Error> {
        let mut allowed_xts = Vec::with_capacity(xts.len());
        let mut allowed_indices = Vec::with_capacity(xts.len());
        let mut final_results: Vec<Option<Result<TxHash<Self>, Self::Error>>> =
            (0..xts.len()).map(|_| None).collect();

        for (i, xt) in xts.into_iter().enumerate() {
            match self.check_banned(&xt) {
                Ok(_) => {
                    allowed_xts.push(xt);
                    allowed_indices.push(i);
                },
                Err(e) => final_results[i] = Some(Err(e.into())),
            }
        }

        if allowed_xts.is_empty() {
            return Box::pin(async move {
                Ok(final_results.into_iter().map(|r| r.expect("All items populated")).collect())
            })
        }

        let inner_future = self.inner.submit_at(at, source, allowed_xts);

        Box::pin(async move {
            let inner_results = inner_future.await?;
            for (result, index) in inner_results.into_iter().zip(allowed_indices) {
                final_results[index] = Some(result);
            }
            Ok(final_results.into_iter().map(|r| r.expect("All items populated")).collect())
        })
    }

    fn submit_one(
        &self,
        at: &BlockId<Self::Block>,
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
        at: &BlockId<Self::Block>,
        source: TransactionSource,
        xt: TransactionFor<Self>,
    ) -> PoolFuture<Pin<Box<sc_transaction_pool_api::TransactionStatusStreamFor<Self>>>, Self::Error>
    {
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
            dyn futures::Future<
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
impl<Pool> MaintainedTransactionPool for FilteredPool<Pool>
where
    Pool: MaintainedTransactionPool,
    Pool::Error: 'static,
{
    async fn maintain(&self, event: ChainEvent<Self::Block>) {
        self.inner.maintain(event).await
    }
}

impl<Pool> sc_transaction_pool_api::LocalTransactionPool for FilteredPool<Pool>
where
    Pool: sc_transaction_pool_api::LocalTransactionPool,
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
///
/// This filter decodes raw extrinsic bytes into a typed call and applies
/// a predicate to determine if the transaction is allowed.
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
    /// Creates a new decoding filter.
    ///
    /// # Arguments
    /// * `enabled` - Whether the filter should actively reject transactions
    /// * `log_rejections` - Whether to log when transactions are rejected
    /// * `decoder` - Function to decode raw bytes into a typed Call
    /// * `predicate` - Function that returns `true` if the Call is allowed
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
