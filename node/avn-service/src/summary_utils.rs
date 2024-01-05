use crate::{extrinsic_utils, merkle_tree_utils, server_error, Config};
use sc_client_api::{client::BlockBackend, UsageProvider};
use sp_core::H256;
use sp_runtime::traits::Block as BlockT;
pub use std::sync::Arc;
use tide::Error as TideError;

pub type EncodedLeafData = Vec<u8>;

pub fn get_extrinsics<Block: BlockT, ClientT>(
    req: &tide::Request<Arc<Config<Block, ClientT>>>,
    from_block_number: u32,
    to_block_number: u32,
) -> Result<Vec<EncodedLeafData>, TideError>
where
    ClientT: BlockBackend<Block> + UsageProvider<Block> + Send + Sync + 'static,
{
    let mut abi_encoded_leaves: Vec<Vec<u8>> = vec![];

    for block_number in from_block_number..=to_block_number {
        let (_, mut extrinsics) =
            extrinsic_utils::process_extrinsics_in_block_and_check_if_filter_target_exists(
                &req.state().client,
                block_number,
                None,
            )
            .map_err(|e| server_error(format!("Error getting extrinsics data: {:?}", e)))?;
        abi_encoded_leaves.append(&mut extrinsics);
    }

    Ok(abi_encoded_leaves)
}

pub fn generate_tree_root(leaves_data: Vec<Vec<u8>>) -> Result<H256, TideError> {
    return merkle_tree_utils::generate_tree_root(leaves_data)
        .map_err(|e| server_error(format!("Error generating merkle root: {:?}", e)))
}
