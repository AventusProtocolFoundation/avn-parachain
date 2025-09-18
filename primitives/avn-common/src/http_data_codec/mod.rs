#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::string::String;

pub fn encode_http_data(data: &Vec<u8>) -> String {
    let encoded: String = hex::encode(data);
    encoded
}
pub fn decode_http_data(encoded: &str) -> Result<Vec<u8>, hex::FromHexError> {
    let trimmed = encoded.trim();
    hex::decode(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_http_data() {
        let data = vec![1, 2, 3, 4, 5];
        let encoded = encode_http_data(&data);
        assert_eq!(encoded, "0102030405");
    }

    #[test]
    fn test_decode_http_data() {
        let encoded = "0102030405";
        let decoded = decode_http_data(encoded).unwrap();
        assert_eq!(decoded, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_encode_decode_http_data() {
        let original_data = vec![10, 20, 30, 40, 50];
        let encoded = encode_http_data(&original_data);
        let decoded = decode_http_data(&encoded).unwrap();
        assert_eq!(decoded, original_data);
    }

    #[test]
    fn test_decode_http_data_with_whitespace() {
        let encoded = "  0102030405  ";
        let decoded = decode_http_data(encoded).unwrap();
        assert_eq!(decoded, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_decode_http_data_invalid_input() {
        let invalid_encoded = "invalid_hex";
        let result = decode_http_data(invalid_encoded);
        assert!(result.is_err());
    }
}
