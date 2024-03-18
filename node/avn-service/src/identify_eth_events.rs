use sp_avn_common::event_types::{Error, EthEvent, EthEventId, EventData, LiftedData};
use web3::types::{FilterBuilder, Log, H160, H256, U64};

#[derive(Default, Clone, PartialEq, Debug, Eq)]
pub struct DiscoveredEvent {
    pub event: EthEvent,
    pub block: u64,
}

pub async fn identify_events(
    eth_node_url: &str,
    start_block: u64,
    end_block: u64,
    contract_addresses: Vec<H160>,
    event_signatures: Vec<H256>,
) -> Result<Vec<DiscoveredEvent>, Error> {
    let web3 = web3::Web3::new(web3::transports::Http::new(eth_node_url).unwrap());

    let filter = FilterBuilder::default()
        .address(contract_addresses)
        .topics(Some(event_signatures), None, None, None)
        .from_block(web3::types::BlockNumber::Number(U64::from(start_block)))
        .to_block(web3::types::BlockNumber::Number(U64::from(end_block)))
        .build();

    let logs = web3.eth().logs(filter).await.unwrap();

    let mut events = Vec::new();

    for log in logs {
        let discovered_event = parse_log(log)?;
        events.push(discovered_event);
    }

    Ok(events)
}

fn parse_log(log: Log) -> Result<DiscoveredEvent, Error> {
    let signature = log.topics[0];
    let transaction_hash = log.transaction_hash.unwrap();
    
    let event_id = EthEventId {
        signature: sp_core::H256::from(signature.0),
        transaction_hash: sp_core::H256::from(transaction_hash.0),
    };

    let event_data = match log.data {
        data => {
            let topics: Vec<Vec<u8>> = log.topics.iter().map(|t| t.0.to_vec()).collect();
            match LiftedData::parse_bytes(Some(data.0), topics) {
                Ok(data) => EventData::LogLifted(data),
                Err(_) => return Err(Error::LiftedEventBadDataLength),
            }
        },
    };

    Ok(DiscoveredEvent {
        event: EthEvent { event_id, event_data },
        block: log.block_number.unwrap().as_u64(),
    })
}
