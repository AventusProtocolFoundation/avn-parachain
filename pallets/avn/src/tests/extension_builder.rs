use crate::mock::TestRuntime;
use codec::alloc::sync::Arc;
use frame_system as system;
use parking_lot::RwLock;
use sp_core::offchain::{
    testing::{OffchainState, PoolState, TestOffchainExt, TestTransactionPoolExt},
    OffchainDbExt as OffchainExt, OffchainWorkerExt, TransactionPoolExt,
};
use sp_io::TestExternalities;

pub type System = system::Pallet<TestRuntime>;

pub struct ExtBuilder {
    pub storage: sp_runtime::Storage,
    offchain_state: Option<Arc<RwLock<OffchainState>>>,
    pool_state: Option<Arc<RwLock<PoolState>>>,
    txpool_extension: Option<TestTransactionPoolExt>,
    offchain_extension: Option<TestOffchainExt>,
    offchain_registered: bool,
}

#[allow(dead_code)]
impl ExtBuilder {
    pub fn build_default() -> Self {
        let storage = system::GenesisConfig::default().build_storage::<TestRuntime>().unwrap();
        Self {
            storage,
            pool_state: None,
            offchain_state: None,
            txpool_extension: None,
            offchain_extension: None,
            offchain_registered: false,
        }
    }

    #[allow(dead_code)]
    pub fn for_offchain_worker(mut self) -> Self {
        assert!(!self.offchain_registered);
        let (offchain, offchain_state) = TestOffchainExt::new();
        let (pool, pool_state) = TestTransactionPoolExt::new();
        self.txpool_extension = Some(pool);
        self.offchain_extension = Some(offchain);
        self.pool_state = Some(pool_state);
        self.offchain_state = Some(offchain_state);
        self.offchain_registered = true;
        self
    }

    #[allow(dead_code)]
    pub fn as_externality(self) -> sp_io::TestExternalities {
        let mut ext = sp_io::TestExternalities::from(self.storage);
        // Events do not get emitted on block 0, so we increment the block here
        ext.execute_with(|| System::set_block_number(1));
        ext
    }

    #[allow(dead_code)]
    pub fn as_externality_with_state(
        self,
    ) -> (TestExternalities, Arc<RwLock<PoolState>>, Arc<RwLock<OffchainState>>) {
        assert!(self.offchain_registered);
        let mut ext = sp_io::TestExternalities::from(self.storage);
        ext.register_extension(OffchainExt::new(self.offchain_extension.clone().unwrap()));
        ext.register_extension(OffchainWorkerExt::new(self.offchain_extension.unwrap()));
        ext.register_extension(TransactionPoolExt::new(self.txpool_extension.unwrap()));
        assert!(self.pool_state.is_some());
        assert!(self.offchain_state.is_some());
        ext.execute_with(|| System::set_block_number(1));
        (ext, self.pool_state.unwrap(), self.offchain_state.unwrap())
    }
}
