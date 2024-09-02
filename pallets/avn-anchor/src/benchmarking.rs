#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::{account, benchmarks, impl_benchmark_test_suite};
use frame_support::BoundedVec;
use frame_system::RawOrigin;
use sp_std::prelude::*;

const SEED: u32 = 0;

benchmarks! {
    register_chain_handler {
        let caller: T::AccountId = account("caller", 0, SEED);
        let name: BoundedVec<u8, ConstU32<32>> = BoundedVec::try_from(vec![0u8; 32]).unwrap();
    }: _(RawOrigin::Signed(caller.clone()), name.clone())
    verify {
        assert!(ChainHandlers::<T>::contains_key(&caller));
        let chain_data = ChainHandlers::<T>::get(&caller).unwrap();
        assert_eq!(chain_data.name, name);
    }

    update_chain_handler {
        let old_handler: T::AccountId = account("old_handler", 0, SEED);
        let new_handler: T::AccountId = account("new_handler", 1, SEED);
        let name: BoundedVec<u8, ConstU32<32>> = BoundedVec::try_from(vec![0u8; 32]).unwrap();

        Pallet::<T>::register_chain_handler(RawOrigin::Signed(old_handler.clone()).into(), name.clone())?;
    }: _(RawOrigin::Signed(old_handler.clone()), new_handler.clone())
    verify {
        assert!(!ChainHandlers::<T>::contains_key(&old_handler));
        assert!(ChainHandlers::<T>::contains_key(&new_handler));
        let chain_data = ChainHandlers::<T>::get(&new_handler).unwrap();
        assert_eq!(chain_data.name, name);
    }

    submit_checkpoint_with_identity {
        let handler: T::AccountId = account("handler", 0, SEED);
        let name: BoundedVec<u8, ConstU32<32>> = BoundedVec::try_from(vec![0u8; 32]).unwrap();
        let checkpoint = H256::random();

        Pallet::<T>::register_chain_handler(RawOrigin::Signed(handler.clone()).into(), name.clone())?;

        let initial_checkpoint_id = NextCheckpointId::<T>::get();
    }: _(RawOrigin::Signed(handler.clone()), checkpoint)
    verify {
        let chain_data = ChainHandlers::<T>::get(&handler).unwrap();
        assert_eq!(Checkpoints::<T>::get(chain_data.chain_id, initial_checkpoint_id), checkpoint);
        assert_eq!(NextCheckpointId::<T>::get(), initial_checkpoint_id + 1);
    }
}

impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::TestRuntime);
