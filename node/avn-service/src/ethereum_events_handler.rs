use sp_avn_common::event_types::{
    AddedValidatorData, AvtGrowthLiftedData, Error, EthEvent, EthEventId, EventData, LiftedData,
    NftCancelListingData, NftEndBatchListingData, NftMintData, NftTransferToData, ValidEvents,
};
use web3::{
    types::{FilterBuilder, Log, H160, H256, U64},
    Web3,
};

#[derive(Debug)]
pub enum AppError {
    ErrorGettingEventLogs,
    MissingTransactionHash,
    MissingBlockNumber,
    ParsingError(Error)
}

pub async fn identify_events(
    web3: &Web3<web3::transports::Http>,
    start_block: u64,
    end_block: u64,
    contract_addresses: Vec<H160>,
    event_signatures: Vec<H256>,
) -> Result<Vec<DiscoveredEvent>, AppError> {
    let filter = FilterBuilder::default()
        .address(contract_addresses)
        .topics(Some(event_signatures), None, None, None)
        .from_block(web3::types::BlockNumber::Number(U64::from(start_block)))
        .to_block(web3::types::BlockNumber::Number(U64::from(end_block)))
        .build();

    let logs_result = web3.eth().logs(filter).await;
    let logs = match logs_result {
        Ok(logs) => logs,
        Err(_) => return Err(AppError::ErrorGettingEventLogs),
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

fn parse_log(log: Log) -> Result<DiscoveredEvent, AppError> {
    let web3_signature = log.topics[0];
    let signature = sp_core::H256::from(web3_signature.0);

    let transaction_hash = match log.transaction_hash {
        Some(transaction_hash) => transaction_hash,
        None => return Err(AppError::MissingTransactionHash),
    };

    let event_id =
        EthEventId { signature, transaction_hash: sp_core::H256::from(transaction_hash.0) };

    let event_data = match signature_to_event_type(signature) {
        Some(event_type) => {
            let topics: Vec<Vec<u8>> = log.topics.iter().map(|t| t.0.to_vec()).collect();
            match parse_event_data(event_type, log.data.0, topics) {
                Ok(data) => data,
                Err(err) => return Err(err),
            }
        },
        None => return Err(AppError::ErrorGettingEventLogs),
    };

    let block_number = log.block_number.ok_or(AppError::MissingBlockNumber)?;

    Ok(DiscoveredEvent { event: EthEvent { event_id, event_data }, block: block_number.as_u64() })
}

fn signature_to_event_type(signature: sp_core::H256) -> Option<ValidEvents> {
    match signature {
        signature if signature == ValidEvents::AddedValidator.signature() =>
            Some(ValidEvents::AddedValidator),
        signature if signature == ValidEvents::Lifted.signature() => Some(ValidEvents::Lifted),
        signature if signature == ValidEvents::NftMint.signature() => Some(ValidEvents::NftMint),
        signature if signature == ValidEvents::NftTransferTo.signature() =>
            Some(ValidEvents::NftTransferTo),
        signature if signature == ValidEvents::NftCancelListing.signature() =>
            Some(ValidEvents::NftCancelListing),
        signature if signature == ValidEvents::NftEndBatchListing.signature() =>
            Some(ValidEvents::NftEndBatchListing),
        signature if signature == ValidEvents::AvtGrowthLifted.signature() =>
            Some(ValidEvents::AvtGrowthLifted),
        _ => None,
    }
}

fn parse_event_data(
    event_type: ValidEvents,
    data: Vec<u8>,
    topics: Vec<Vec<u8>>,
) -> Result<EventData, AppError> {
    match event_type {
        ValidEvents::AddedValidator => {
            AddedValidatorData::parse_bytes(Some(data), topics)
                .map_err(|err| AppError::ParsingError(err.into()))
                .map(EventData::LogAddedValidator)
        }
        ValidEvents::Lifted => {
            LiftedData::parse_bytes(Some(data), topics)
                .map_err(|err| AppError::ParsingError(err.into()))
                .map(EventData::LogLifted)
        }
        ValidEvents::NftMint => {
            NftMintData::parse_bytes(Some(data), topics)
                .map_err(|err| AppError::ParsingError(err.into()))
                .map(EventData::LogNftMinted)
        }
        ValidEvents::NftTransferTo => {
            NftTransferToData::parse_bytes(Some(data), topics)
                .map_err(|err| AppError::ParsingError(err))
                .map(EventData::LogNftTransferTo)
        }
        ValidEvents::NftCancelListing => {
            NftCancelListingData::parse_bytes(Some(data), topics)
                .map_err(|err| AppError::ParsingError(err.into()))
                .map(EventData::LogNftCancelListing)
        }
        ValidEvents::NftEndBatchListing => {
            NftEndBatchListingData::parse_bytes(Some(data), topics)
                .map_err(|err| AppError::ParsingError(err.into()))
                .map(EventData::LogNftEndBatchListing)
        }
        ValidEvents::AvtGrowthLifted => {
            AvtGrowthLiftedData::parse_bytes(Some(data), topics)
                .map_err(|err| AppError::ParsingError(err.into()))
                .map(EventData::LogAvtGrowthLifted)
        }
    }
}
