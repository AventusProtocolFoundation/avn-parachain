use hex_literal::hex;
use sp_core::ecdsa::Public;

#[cfg(test)]
use super::*;

pub fn compressed_key() -> Public {
    Public::from_raw(hex!("02407b0d9f41148bbe3b6c7d4a62585ae66cc32a707441197fa5453abfebd31d57"))
}

pub fn expected_decompressed_key() -> H512 {
    H512::from_slice(
        hex!(
            "407b0d9f41148bbe3b6c7d4a62585ae66cc32a707441197fa5453abfebd31d57162f3d20faa2b513964472d2f8d4b585330c565a5696e1829a537bb2856c0dbc"
        ).as_slice()
    )
}

#[test]
fn test_decompress_eth_public_key() {
    let compressed_key = compressed_key();
    let expected_decompressed_key = expected_decompressed_key();

    let decompressed_key = decompress_eth_public_key(compressed_key);

    match decompressed_key {
        Ok(key) => assert_eq!(key, expected_decompressed_key),
        Err(e) => {
            panic!("decompress_eth_public_key failed with error: {:?}", e);
        },
    }
}
