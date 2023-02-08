use hex::FromHex;
use simple_json2::{
    impls::SimpleError,
    json::{JsonObject, JsonValue},
    parser::Error,
};
use sp_core::{H160, H256};
use sp_std::prelude::*;

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::string::String;

// TODO [TYPE: refactoring][PRI: unknown][NOTE: clarify]: extend the parser to work with named
// properties
const INDEX_EVENT_SIGNATURE_TOPIC: usize = 0;

fn get_value_of(key: String, object: &JsonObject) -> Result<&JsonValue, SimpleError> {
    let key_char: Vec<char> = key.chars().collect();
    let value = object.into_iter().find(|v| v.0 == key_char);
    if let Some(value) = value {
        return Ok(&value.1)
    }

    return Err(SimpleError::plain_str("key not found in object"))
}

fn get_result(json_response: &JsonValue) -> Result<&JsonObject, SimpleError> {
    let response = json_response.get_object()?;
    if response.len() == 0 {
        return Err(SimpleError::plain_str("empty response"))
    }
    let result = get_value_of(String::from("result"), response)?.get_object()?;
    return Ok(result)
}

// TODO: Refactor to use fn get_result.
pub fn get_events(json_response: &JsonValue) -> Result<&Vec<JsonValue>, SimpleError> {
    let response = json_response.get_object()?;
    if response.len() == 0 {
        return Err(SimpleError::plain_str("empty response"))
    }
    let result = get_value_of(String::from("result"), response)?.get_object()?;
    let events = get_value_of(String::from("logs"), result)?.get_array()?;

    return Ok(events)
}

pub fn find_event(events: &Vec<JsonValue>, topic: H256) -> Option<(&JsonValue, H160)> {
    let event = events
        .into_iter()
        .find(|event| topic_matches(event, topic).map_or_else(|_| false, |v| v));

    if let Some(event) = event {
        let contract_address = get_contract_address(event);

        if let Ok(contract_address) = contract_address {
            return Some((event, contract_address))
        }
    }

    return None
}

pub fn get_data(event: &JsonValue) -> Result<Option<Vec<u8>>, SimpleError> {
    let event = event.get_object()?;
    let data = get_value_of(String::from("data"), event)?.get_string()?;
    let bytes = hex_to_bytes(data)?;

    if bytes.len() > 0 {
        return Ok(Some(bytes))
    }

    return Ok(None)
}

pub fn get_num_confirmations(json_response: &JsonValue) -> Result<u64, SimpleError> {
    let num_confirmations =
        get_value_of(String::from("num_confirmations"), json_response.get_object()?)?
            .get_number_f64()?;
    return Ok(num_confirmations as u64)
}

pub fn get_status(json_response: &JsonValue) -> Result<u8, SimpleError> {
    let result = get_result(json_response)?;
    let status = get_value_of(String::from("status"), result)?.get_string()?;
    match u8::from_str_radix(status.trim_start_matches("0x"), 16) {
        Ok(s) => Ok(s),
        Err(_e) => Err(SimpleError::plain_str("Status is not a valid hex number")),
    }
}

pub fn get_topics(event: &JsonValue) -> Result<Vec<Vec<u8>>, SimpleError> {
    let event = event.get_object()?;
    let topics = get_value_of(String::from("topics"), event)?.get_array()?;
    let mut topics_bytes: Vec<Vec<u8>> = Vec::<Vec<u8>>::new();
    for topic in topics.into_iter() {
        let topic_string = topic.get_string()?;
        let topic_bytes = hex_to_bytes(topic_string)?;
        topics_bytes.push(topic_bytes);
    }

    return Ok(topics_bytes)
}

fn get_event_signature(event: &JsonValue) -> Result<String, SimpleError> {
    let event = event.get_object()?;
    let topics = get_value_of(String::from("topics"), event)?.get_array()?;
    let event_signature = topics[INDEX_EVENT_SIGNATURE_TOPIC].get_string()?;

    return Ok(event_signature)
}

fn get_contract_address(event: &JsonValue) -> Result<H160, SimpleError> {
    let event = event.get_object()?;
    let address = get_value_of(String::from("address"), event)?.get_string()?;
    let bytes = hex_to_bytes(address)?;

    return Ok(H160::from_slice(&bytes))
}

fn hex_to_bytes(hex_string: String) -> Result<Vec<u8>, SimpleError> {
    let mut hex_string = hex_string.to_lowercase();
    if hex_string.starts_with("0x") {
        hex_string = hex_string[2..].into();
    }

    return Vec::from_hex(hex_string)
        .map_or_else(|_error| Err(SimpleError::plain_str("hex_to_bytes error")), |bytes| Ok(bytes))
}

fn to_bytes32(hex_topic: String) -> Result<[u8; 32], SimpleError> {
    let mut hex_topic = hex_topic.to_lowercase();
    if hex_topic.starts_with("0x") {
        hex_topic = hex_topic[2..].into();
    }

    return <[u8; 32]>::from_hex(hex_topic).map_or_else(
        |_error| Err(SimpleError::plain_str("to_bytes32 error")),
        |bytes32| Ok(bytes32),
    )
}

fn topic_matches(event: &JsonValue, topic: H256) -> Result<bool, SimpleError> {
    let event_signature_bytes = to_bytes32(get_event_signature(event)?)?;
    return Ok(H256(event_signature_bytes) == topic)
}

#[cfg(test)]
#[path = "tests/test_event_parser.rs"]
mod test_event_parser;
