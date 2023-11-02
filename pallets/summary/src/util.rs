use super::*;
use ethabi;
use sp_core::U256;
pub fn u256_to_big_endian(value: &U256) -> [u8; 32] {
    let mut uint256 = [0u8; 32];
    value.to_big_endian(&mut uint256[..]);
    uint256
}

pub fn encode_summary_data(
    hash_data: &[u8; 32],
    expiry: u64,
    transaction_id: EthereumTransactionId,
) -> Vec<u8> {
    let call_values: Vec<ethabi::Token> = vec![
        ethabi::Token::FixedBytes(hash_data.to_vec()),
        ethabi::Token::Uint(u256_to_big_endian(&U256::from(expiry)).into()),
        ethabi::Token::Uint(u256_to_big_endian(&U256::from(transaction_id)).into()),
    ];
    ethabi::encode(&call_values)
}
