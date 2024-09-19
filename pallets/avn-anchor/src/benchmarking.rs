#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::{
    encode_signed_register_chain_handler_params, encode_signed_submit_checkpoint_params,
    encode_signed_update_chain_handler_params,
};
use codec::{Decode, Encode};
use frame_benchmarking::{account, benchmarks, impl_benchmark_test_suite};
use frame_support::BoundedVec;
use frame_system::RawOrigin;
use sp_application_crypto::KeyTypeId;
use sp_avn_common::Proof;
use sp_core::H256;
use sp_runtime::RuntimeAppPublic;

pub const BENCH_KEY_TYPE_ID: KeyTypeId = KeyTypeId(*b"test");

mod app_sr25519 {
    use super::BENCH_KEY_TYPE_ID;
    use sp_application_crypto::{app_crypto, sr25519};
    app_crypto!(sr25519, BENCH_KEY_TYPE_ID);
}

type SignerId = app_sr25519::Public;

const SEED: u32 = 0;

fn create_account_id<T: Config>(seed: u32) -> T::AccountId {
    account("account", seed, SEED)
}

fn create_proof<T: Config>(
    signature: sp_core::sr25519::Signature,
    signer: T::AccountId,
    relayer: T::AccountId,
) -> Proof<T::Signature, T::AccountId> {
    Proof { signer, relayer, signature: signature.into() }
}

benchmarks! {
    register_chain_handler {
        let caller: T::AccountId = account("caller", 0, SEED);
        let name: BoundedVec<u8, ConstU32<32>> = BoundedVec::try_from(vec![0u8; 32]).unwrap();
    }: _(RawOrigin::Signed(caller.clone()), name.clone())
    verify {
        assert!(ChainHandlers::<T>::contains_key(&caller));
        let chain_data = ChainData::<T>::get(ChainHandlers::<T>::get(&caller).unwrap()).unwrap();
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
        let chain_data = ChainData::<T>::get(ChainHandlers::<T>::get(&new_handler).unwrap()).unwrap();
        assert_eq!(chain_data.name, name);
    }

    submit_checkpoint_with_identity {
        let handler: T::AccountId = account("handler", 0, SEED);
        let name: BoundedVec<u8, ConstU32<32>> = BoundedVec::try_from(vec![0u8; 32]).unwrap();
        let checkpoint = H256::from([0u8; 32]);

        Pallet::<T>::register_chain_handler(RawOrigin::Signed(handler.clone()).into(), name.clone())?;

        let chain_id = ChainHandlers::<T>::get(&handler).unwrap();
        let initial_checkpoint_id = NextCheckpointId::<T>::get(chain_id);
    }: _(RawOrigin::Signed(handler.clone()), checkpoint)
    verify {
        assert_eq!(Checkpoints::<T>::get(chain_id, initial_checkpoint_id), checkpoint);
        assert_eq!(NextCheckpointId::<T>::get(chain_id), initial_checkpoint_id + 1);
    }

    signed_register_chain_handler {
        let signer_pair = SignerId::generate_pair(None);
        let handler: T::AccountId = T::AccountId::decode(&mut signer_pair.encode().as_slice())
            .expect("Valid account id");
        let relayer: T::AccountId = create_account_id::<T>(1);
        let name: BoundedVec<u8, ConstU32<32>> = BoundedVec::try_from(vec![0u8; 32]).unwrap();
        let nonce = 0;

        let payload = encode_signed_register_chain_handler_params::<T>(&relayer, &handler, &name);
        let signature = signer_pair.sign(&payload).ok_or("Error signing proof")?;
        let proof = create_proof::<T>(signature.into(), handler.clone(), relayer);
    }: _(RawOrigin::Signed(handler.clone()), proof, handler.clone(), name.clone())
    verify {
        assert!(ChainHandlers::<T>::contains_key(&handler));
        let chain_data = ChainData::<T>::get(ChainHandlers::<T>::get(&handler).unwrap()).unwrap();
        assert_eq!(chain_data.name, name);
    }

    signed_update_chain_handler {
        let old_signer_pair = SignerId::generate_pair(None);
        let old_handler: T::AccountId = T::AccountId::decode(&mut old_signer_pair.encode().as_slice())
            .expect("Valid account id");
        let new_handler: T::AccountId = create_account_id::<T>(1);
        let relayer: T::AccountId = create_account_id::<T>(2);
        let name: BoundedVec<u8, ConstU32<32>> = BoundedVec::try_from(vec![0u8; 32]).unwrap();

        Pallet::<T>::register_chain_handler(RawOrigin::Signed(old_handler.clone()).into(), name.clone())?;
        let chain_id = ChainHandlers::<T>::get(&old_handler).unwrap();
        let nonce = Nonces::<T>::get(chain_id);

        let payload = encode_signed_update_chain_handler_params::<T>(
            &relayer,
            &old_handler,
            &new_handler,
            chain_id,
            nonce
        );
        let signature = old_signer_pair.sign(&payload).ok_or("Error signing proof")?;
        let proof = create_proof::<T>(signature.into(), old_handler.clone(), relayer);
    }: _(RawOrigin::Signed(old_handler.clone()), proof, old_handler.clone(), new_handler.clone())
    verify {
        assert!(!ChainHandlers::<T>::contains_key(&old_handler));
        assert!(ChainHandlers::<T>::contains_key(&new_handler));
        let chain_data = ChainData::<T>::get(ChainHandlers::<T>::get(&new_handler).unwrap()).unwrap();
        assert_eq!(chain_data.name, name);
    }

    signed_submit_checkpoint_with_identity {
        let signer_pair = SignerId::generate_pair(None);
        let handler: T::AccountId = T::AccountId::decode(&mut Encode::encode(&signer_pair).as_slice()).expect("valid account id");
        let relayer: T::AccountId = create_account_id::<T>(1);
        let name: BoundedVec<u8, ConstU32<32>> = BoundedVec::try_from(vec![0u8; 32]).unwrap();
        let checkpoint = H256::from([0u8; 32]);

        Pallet::<T>::register_chain_handler(RawOrigin::Signed(handler.clone()).into(), name.clone())?;
        let chain_id = ChainHandlers::<T>::get(&handler).unwrap();
        let nonce = Nonces::<T>::get(chain_id);
        let initial_checkpoint_id = NextCheckpointId::<T>::get(chain_id);

        let payload = encode_signed_submit_checkpoint_params::<T>(&relayer, &handler, &checkpoint, chain_id, nonce);
        let signature = signer_pair.sign(&payload).ok_or("Error signing proof")?;
        let proof = create_proof::<T>(signature.into(), handler.clone(), relayer);
    }: _(RawOrigin::Signed(handler.clone()), proof, handler.clone(), checkpoint)
    verify {
        assert_eq!(Checkpoints::<T>::get(chain_id, initial_checkpoint_id), checkpoint);
        assert_eq!(NextCheckpointId::<T>::get(chain_id), initial_checkpoint_id + 1);
    }
}

impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::TestRuntime);
