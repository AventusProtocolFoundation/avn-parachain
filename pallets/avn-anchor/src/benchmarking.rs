#![cfg(feature = "runtime-benchmarks")]

use super::*;

use crate::{Pallet as AvnAnchor, *};
use frame_benchmarking::{account, benchmarks, impl_benchmark_test_suite};
use frame_system::{EventRecord, RawOrigin};
use sp_runtime::traits::Bounded;

const SEED: u32 = 0;

benchmarks! {
    register_chain_handler {
        let caller: T::AccountId = account("known_sender", 1, 1);
        let chain_id: ChainId = 1;
        let handler: T::AccountId = account("handler", 0, SEED);
    }: _(RawOrigin::Signed(caller), chain_id, handler.clone())
    verify {
        assert_eq!(ChainHandlers::<T>::get(chain_id), Some(handler));
    }

    update_chain_handler {
        let caller: T::AccountId = account("known_sender", 1, 1);
        let chain_id: ChainId = 1;
        let initial_handler: T::AccountId = account("initial_handler", 0, SEED);
        let new_handler: T::AccountId = account("new_handler", 1, SEED);

        Pallet::<T>::register_chain_handler(RawOrigin::Signed(caller.clone()).into(), chain_id, initial_handler)?;
    }: _(RawOrigin::Signed(caller), chain_id, new_handler.clone())
    verify {
        assert_eq!(ChainHandlers::<T>::get(chain_id), Some(new_handler));
    }
}

impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::TestRuntime,);
