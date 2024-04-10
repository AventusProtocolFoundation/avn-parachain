#![cfg_attr(not(feature = "std"), no_std)]
use codec::Codec;
use frame_support::dispatch::Vec;
use sp_avn_common::event_discovery::{EthBlockRange, EthereumEventsPartition};
use sp_core::{H160, H256};

sp_api::decl_runtime_apis! {

    #[api_version(1)]
    pub trait EthEventHandlerApi<AccountId>
            where
        AccountId: Codec,
    {
        fn query_active_block_range()-> (EthBlockRange, u16);
        fn query_has_author_casted_event_vote(account_id: AccountId) -> bool;
        fn query_signatures() -> Vec<H256>;
        fn query_bridge_contract() -> H160;
        fn create_proof(account_id:AccountId, events_partition:EthereumEventsPartition)-> Vec<u8>;
        fn submit_vote(
            author: AccountId,
            events_partition: EthereumEventsPartition,
            signature: sp_core::sr25519::Signature
        ) -> Result<(), ()>;
    }
}
