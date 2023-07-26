use codec::Codec;
use sc_client_api::{client::BlockBackend, UsageProvider};
use sp_runtime::traits::Block as BlockT;
use std::sync::Arc;

use jsonrpsee::{
	core::{Error as JsonRpseeError, RpcResult as Result},
	proc_macros::rpc,
	types::error::{CallError, ErrorCode, ErrorObject},
};

use avn_service::{extrinsic_utils::*, merkle_tree_utils::*, Error as AvnServiceError};
use node_primitives::AccountId;

#[rpc(server)]
pub trait LowerDataProviderRpc {
	#[method(name = "lower_data", blocking)]
	fn get_lower_data(
		&self,
		from_block: u32,
		to_block: u32,
		block_number: u32,
		extrinsic_index: u32,
	) -> Result<String>;
}

pub struct LowerDataProvider<C, Block> {
	client: Arc<C>,
	_marker: std::marker::PhantomData<Block>,
}

impl<C, Block> LowerDataProvider<C, Block> {
	pub fn new(client: Arc<C>) -> Self {
		Self { client, _marker: Default::default() }
	}
}

impl<C, Block> LowerDataProviderRpcServer for LowerDataProvider<C, Block>
where
	Block: BlockT,
	C: Send + Sync + 'static + BlockBackend<Block> + UsageProvider<Block>,
	AccountId: Clone + std::fmt::Display + Codec,
{
	fn get_lower_data(
		&self,
		from_block: u32,
		to_block: u32,
		block_number: u32,
		extrinsic_index: u32,
	) -> Result<String> {
		let leaf_filter: LowerLeafFilter = LowerLeafFilter { block_number, extrinsic_index };

		let (encoded_leaf, extrinsics) = get_extrinsics_and_check_if_filter_target_exists(
			&self.client,
			from_block,
			to_block,
			leaf_filter,
		)?;

		if extrinsics.len() > 0 && encoded_leaf.is_some() {
			let leaf = encoded_leaf.expect("Leaf exists");
			let merkle_path = generate_merkle_path(&leaf, extrinsics)?;
			let response = MerklePathData { encoded_leaf: leaf, merkle_path };

			return Ok(hex::encode(serde_json::to_string(&response).map_err(|e| {
				JsonRpseeError::Call(CallError::Custom(ErrorObject::owned(
					ErrorCode::ServerError(AvnServiceError::ResponseError.into()).code(),
					"Error converting response to string",
					Some(format!("{:?}", e)),
				)))
			})?))
		}

		// the leaf is missing or the filter values are incorrect
		Ok(hex::encode("".to_string()))
	}
}
