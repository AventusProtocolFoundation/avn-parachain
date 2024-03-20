use sp_avn_common::event_types::{Error, EthEvent, EthEventId, EventData, LiftedData};
use web3::{types::{FilterBuilder, Log, H160, H256, U64}, Web3};

#[derive(Default, Clone, PartialEq, Debug, Eq)]
pub struct DiscoveredEvent {
    pub event: EthEvent,
    pub block: u64,
}

pub async fn identify_events(
    web3: &Web3<web3::transports::Http>,
    start_block: u64,
    end_block: u64,
    contract_addresses: Vec<H160>,
    event_signatures: Vec<H256>,
) -> Result<Vec<DiscoveredEvent>, Error> {

    let filter = FilterBuilder::default()
        .address(contract_addresses)
        .topics(Some(event_signatures), None, None, None)
        .from_block(web3::types::BlockNumber::Number(U64::from(start_block)))
        .to_block(web3::types::BlockNumber::Number(U64::from(end_block)))
        .build();

        let logs_result = web3.eth().logs(filter).await;
        let logs = match logs_result {
            Ok(logs) => logs,
            Err(err) => return Err(Error::NftTransferToEventWrongTopicCount)
        };

    let mut events = Vec::new();

    for log in logs {
        match parse_log(log) {
            Ok(discovered_event) => events.push(discovered_event),
            Err(err) => return Err(err),
        }
    }

    Ok(events)
}

fn parse_log(log: Log) -> Result<DiscoveredEvent, Error> {
    let signature = log.topics[0];
    let transaction_hash = match log.transaction_hash {
        Some(transaction_hash) => transaction_hash,
        None => return Err(Error::MissingTransactionHash),
    };
    
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

    let block_number = match log.block_number {
        Some(block_number) =>block_number,
        None => return Err(Error::MissingBlockNumber)
    };

    Ok(DiscoveredEvent {
        event: EthEvent { event_id, event_data },
        block: block_number.as_u64(),
    })
}
