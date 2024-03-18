use web3::{
    types::{FilterBuilder, H160, H256, U64},
};

pub async fn identify_events(
    eth_node_url: &str,
    start_block: u64,
    end_block: u64,
    contract_addresses: Vec<H160>,
    event_signatures: Vec<H256>,
) -> Result<Vec<web3::types::Log>, web3::Error> {
    let web3 = web3::Web3::new(web3::transports::Http::new(eth_node_url)?);

    let filter = FilterBuilder::default()
        .address(contract_addresses)
        .topics(Some(event_signatures), None, None, None)
        .from_block(web3::types::BlockNumber::Number(U64::from(start_block)))
        .to_block(web3::types::BlockNumber::Number(U64::from(end_block)))
        .build();

    let logs = web3.eth().logs(filter).await?;

    Ok(logs)
}
