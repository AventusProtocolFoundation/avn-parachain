#![cfg_attr(not(feature = "std"), no_std)]
use codec::Codec;
use sp_api::vec::Vec;
use sp_avn_common::event_discovery::{AdditionalEvents, EthBlockRange, EthereumEventsPartition};
use sp_core::{H160, H256};

sp_api::decl_runtime_apis! {

    #[api_version(1)]
    pub trait EthEventHandlerApi<AccountId>
            where
        AccountId: Codec,
    {
        fn query_authors() -> Vec<([u8; 32], [u8; 32])>;
        fn query_active_block_range()-> Option<(EthBlockRange, u16)>;
        fn query_has_author_casted_vote(account_id: AccountId) -> bool;
        fn query_signatures() -> Vec<H256>;
        fn query_bridge_contract() -> H160;
        fn submit_vote(
            author: AccountId,
            events_partition: EthereumEventsPartition,
            signature: sp_core::sr25519::Signature
        ) -> Option<()>;
        fn submit_latest_ethereum_block(
            author: AccountId,
            latest_seen_block: u32,
            signature: sp_core::sr25519::Signature
        ) -> Option<()>;
        fn partition_has_additional_events() -> Option<AdditionalEvents>;
    }
}
