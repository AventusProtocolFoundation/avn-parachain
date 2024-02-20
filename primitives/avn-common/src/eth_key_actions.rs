use sp_core::{ecdsa, H512};

pub fn decompress_eth_public_key(
    compressed_eth_public_key: ecdsa::Public,
) -> Result<H512, libsecp256k1::Error> {
    let decompressed = libsecp256k1::PublicKey::parse_slice(
        &compressed_eth_public_key.0,
        Some(libsecp256k1::PublicKeyFormat::Compressed),
    );
    match decompressed {
        Ok(public_key) => {
            let decompressed = public_key.serialize();
            let mut m = [0u8; 64];
            m.copy_from_slice(&decompressed[1..65]);
            Ok(H512::from_slice(&m))
        },
        Err(err) => Err(err),
    }
}

#[cfg(test)]
#[path = "tests/test_eth_key_actions.rs"]
mod test_eth_key_actions;
