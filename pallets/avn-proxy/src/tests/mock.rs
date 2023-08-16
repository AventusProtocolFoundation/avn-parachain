//Copyright 2022 Aventus Network Services (UK) Ltd.

use super::*;
use frame_support::parameter_types;
use frame_system as system;
use hex_literal::hex;
use pallet_balances;
use pallet_nft_manager::nft_data::Royalty;
use sp_core::{sr25519, Pair, H160, H256};
use sp_keystore::{testing::KeyStore, KeystoreExt};
use sp_runtime::{
    testing::{Header, UintAuthorityId},
    traits::{BlakeTwo256, IdentityLookup, Verify},
};
pub use std::sync::Arc;

pub const ONE_AVT: u128 = 1_000000_000000_000000u128;
pub const HUNDRED_AVT: u128 = 100 * ONE_AVT;
pub const EXISTENTIAL_DEPOSIT: u64 = 0;

/// The signature type used by accounts/transactions.
pub type Signature = sr25519::Signature;
/// An identifier for an account on this system.
pub type AccountId = <Signature as Verify>::Signer;

use crate::{self as avn_proxy};
frame_support::construct_runtime!(
    pub enum TestRuntime where
        Block = Block,
        NodeBlock = Block,
        UncheckedExtrinsic = UncheckedExtrinsic,
    {
        System: frame_system::{Pallet, Call, Config, Storage, Event<T>},
        Balances: pallet_balances::{Pallet, Call, Storage, Event<T>},
        NftManager: pallet_nft_manager::{Pallet, Call, Storage, Event<T>},
        AvnProxy: avn_proxy::{Pallet, Call, Storage, Event<T>},
    }
);

impl Config for TestRuntime {
    type RuntimeEvent = mock::RuntimeEvent;
    type RuntimeCall = RuntimeCall;
    type Currency = Balances;
    type Public = AccountId;
    type Signature = Signature;
    type ProxyConfig = TestAvnProxyConfig;
    type WeightInfo = ();
}

pub type AvnProxyCall = super::Call<TestRuntime>;
pub type SystemCall = frame_system::Call<TestRuntime>;
pub type BalancesCall = pallet_balances::Call<TestRuntime>;
pub type NftManagerCall = pallet_nft_manager::Call<TestRuntime>;
pub type Hashing = <TestRuntime as system::Config>::Hashing;
type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<TestRuntime>;
type Block = frame_system::mocking::MockBlock<TestRuntime>;

impl sp_runtime::BoundToRuntimeAppPublic for TestRuntime {
    type Public = <mock::TestRuntime as pallet_avn::Config>::AuthorityId;
}

parameter_types! {
    pub const BlockHashCount: u64 = 250;
}

impl system::Config for TestRuntime {
    type BaseCallFilter = frame_support::traits::Everything;
    type BlockWeights = ();
    type BlockLength = ();
    type DbWeight = ();
    type RuntimeOrigin = RuntimeOrigin;
    type RuntimeCall = RuntimeCall;
    type Index = u64;
    type BlockNumber = u64;
    type Hash = H256;
    type Hashing = BlakeTwo256;
    type AccountId = AccountId;
    type Lookup = IdentityLookup<Self::AccountId>;
    type Header = Header;
    type RuntimeEvent = RuntimeEvent;
    type BlockHashCount = BlockHashCount;
    type Version = ();
    type PalletInfo = PalletInfo;
    type AccountData = pallet_balances::AccountData<u128>;
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type SystemWeightInfo = ();
    type SS58Prefix = ();
    type OnSetCode = ();
    type MaxConsumers = frame_support::traits::ConstU32<16>;
}

parameter_types! {
    pub const ExistentialDeposit: u64 = EXISTENTIAL_DEPOSIT;
}

impl pallet_balances::Config for TestRuntime {
    type MaxLocks = frame_support::traits::ConstU32<1024>;
    type MaxReserves = ();
    type ReserveIdentifier = [u8; 8];
    type Balance = u128;
    type RuntimeEvent = RuntimeEvent;
    type DustRemoval = ();
    type ExistentialDeposit = ExistentialDeposit;
    type AccountStore = System;
    type WeightInfo = ();
}

impl pallet_nft_manager::Config for TestRuntime {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeCall = RuntimeCall;
    type ProcessedEventsChecker = ();
    type Public = AccountId;
    type Signature = Signature;
    type WeightInfo = ();
}

impl pallet_avn::Config for TestRuntime {
    type AuthorityId = UintAuthorityId;
    type EthereumPublicKeyChecker = ();
    type NewSessionHandler = ();
    type DisabledValidatorChecker = ();
    type FinalisedBlockChecker = ();
}

// Test Avn proxy configuration logic
// We only allow System::Remark and signed_mint_single_nft calls to be proxied
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Encode, Decode, Debug, TypeInfo)]
pub struct TestAvnProxyConfig {}
impl Default for TestAvnProxyConfig {
    fn default() -> Self {
        TestAvnProxyConfig {}
    }
}

impl ProvableProxy<RuntimeCall, Signature, AccountId> for TestAvnProxyConfig {
    fn get_proof(call: &RuntimeCall) -> Option<Proof<Signature, AccountId>> {
        match call {
            RuntimeCall::System(system::Call::remark { remark: _msg }) => {
                let context: ProxyContext = Default::default();
                return Some(context.get_proof())
            },

            RuntimeCall::NftManager(pallet_nft_manager::Call::signed_mint_single_nft {
                proof,
                unique_external_ref: _,
                royalties: _,
                t1_authority: _,
            }) => return Some(proof.clone()),
            _ => None,
        }
    }
}

impl InnerCallValidator for TestAvnProxyConfig {
    type Call = RuntimeCall;

    fn signature_is_valid(call: &Box<Self::Call>) -> bool {
        match **call {
            RuntimeCall::System(..) => return true,
            RuntimeCall::NftManager(..) =>
                return pallet_nft_manager::Pallet::<TestRuntime>::signature_is_valid(call),
            _ => false,
        }
    }
}

// ==============================================================================================

pub struct ExtBuilder {
    storage: sp_runtime::Storage,
}

impl ExtBuilder {
    pub fn build_default() -> Self {
        let storage = system::GenesisConfig::default().build_storage::<TestRuntime>().unwrap();
        Self { storage }
    }

    pub fn as_externality(self) -> sp_io::TestExternalities {
        let keystore = KeyStore::new();

        let mut ext = sp_io::TestExternalities::from(self.storage);
        ext.register_extension(KeystoreExt(Arc::new(keystore)));
        // Events do not get emitted on block 0, so we increment the block here
        ext.execute_with(|| System::set_block_number(1));
        ext
    }

    pub fn with_balances(mut self) -> Self {
        let context: ProxyContext = Default::default();

        let _ = pallet_balances::GenesisConfig::<TestRuntime> {
            balances: vec![
                (context.signer.account_id(), HUNDRED_AVT),
                (context.relayer.account_id(), HUNDRED_AVT),
            ],
        }
        .assimilate_storage(&mut self.storage);
        self
    }
}

// ============================= Signature handling ========================

// TODO: Refactor this struct to be reused in all tests
#[derive(Clone, Copy)]
pub struct TestAccount {
    pub seed: [u8; 32],
}

impl TestAccount {
    pub fn new(seed: [u8; 32]) -> Self {
        TestAccount { seed }
    }

    pub fn account_id(&self) -> AccountId {
        return AccountId::decode(&mut self.key_pair().public().to_vec().as_slice()).unwrap()
    }

    pub fn key_pair(&self) -> sr25519::Pair {
        return sr25519::Pair::from_seed(&self.seed)
    }
}

pub fn sign(signer: &sr25519::Pair, message_to_sign: &[u8]) -> Signature {
    return Signature::from(signer.sign(message_to_sign))
}

#[allow(dead_code)]
pub fn verify_signature(signature: Signature, signer: AccountId, signed_data: &[u8]) -> bool {
    return signature.verify(signed_data, &signer)
}

#[derive(Clone)]
pub struct ProxyContext {
    pub signer: TestAccount,
    pub relayer: TestAccount,
    pub signature: Signature,
}

impl Default for ProxyContext {
    fn default() -> Self {
        let message = &("").encode();
        let signer = TestAccount::new([1u8; 32]);
        ProxyContext {
            signer,
            relayer: TestAccount::new([10u8; 32]),
            signature: sign(&signer.key_pair(), message),
        }
    }
}

impl ProxyContext {
    pub fn get_proof(&self) -> Proof<Signature, AccountId> {
        return Proof {
            signer: self.signer.account_id(),
            relayer: self.relayer.account_id(),
            signature: self.signature.clone(),
        }
    }

    pub fn create_valid_inner_call(&self) -> Box<<TestRuntime as Config>::RuntimeCall> {
        return Box::new(RuntimeCall::System(SystemCall::remark { remark: vec![] }))
    }

    pub fn create_invalid_inner_call(&self) -> Box<<TestRuntime as Config>::RuntimeCall> {
        let invalid_receiver = TestAccount::new([8u8; 32]);
        return Box::new(RuntimeCall::Balances(BalancesCall::transfer {
            dest: invalid_receiver.account_id(),
            value: Default::default(),
        }))
    }

    pub fn create_proxy_call(&self) -> Box<<TestRuntime as Config>::RuntimeCall> {
        return Box::new(RuntimeCall::AvnProxy(AvnProxyCall::proxy {
            call: self.create_valid_inner_call(),
            payment_info: None,
        }))
    }
}

pub fn proxy_event_emitted(
    relayer: AccountId,
    call_hash: <TestRuntime as system::Config>::Hash,
) -> bool {
    return System::events().iter().any(|a| {
        a.event ==
            RuntimeEvent::AvnProxy(crate::Event::<TestRuntime>::CallDispatched {
                relayer,
                hash: call_hash,
            })
    })
}

pub fn inner_call_failed_event_emitted(
    call_relayer: AccountId,
    call_hash: <TestRuntime as system::Config>::Hash,
) -> bool {
    return System::events().iter().any(|a| match a.event {
        RuntimeEvent::AvnProxy(crate::Event::<TestRuntime>::InnerCallFailed {
            relayer,
            hash,
            ..
        }) =>
            if relayer == call_relayer && call_hash == hash {
                return true
            } else {
                return false
            },
        _ => false,
    })
}

#[derive(Clone)]
pub struct SingleNftContext {
    pub unique_external_ref: Vec<u8>,
    pub royalties: Vec<Royalty>,
    pub t1_authority: H160,
}

impl Default for SingleNftContext {
    fn default() -> Self {
        let t1_authority = H160(hex!("0000000000000000000000000000000000000001"));
        let royalties: Vec<Royalty> = vec![];
        let unique_external_ref = String::from("Offchain location of NFT").into_bytes();
        SingleNftContext { unique_external_ref, royalties, t1_authority }
    }
}

pub const SIGNED_MINT_SINGLE_NFT_CONTEXT: &'static [u8] =
    b"authorization for mint single nft operation";

pub fn create_signed_mint_single_nft_call(
    context: &ProxyContext,
) -> Box<<TestRuntime as Config>::RuntimeCall> {
    let single_nft_data: SingleNftContext = Default::default();
    let proof = get_mint_single_nft_proxy_proof(context, &single_nft_data);

    return get_signed_mint_single_nft_call(&single_nft_data, &proof)
}

pub fn get_signed_mint_single_nft_call(
    single_nft_data: &SingleNftContext,
    proof: &Proof<Signature, AccountId>,
) -> Box<<TestRuntime as Config>::RuntimeCall> {
    return Box::new(crate::mock::RuntimeCall::NftManager(NftManagerCall::signed_mint_single_nft {
        proof: proof.clone(),
        unique_external_ref: single_nft_data.unique_external_ref.clone(),
        royalties: single_nft_data.royalties.clone(),
        t1_authority: single_nft_data.t1_authority,
    }))
}

pub fn get_mint_single_nft_proxy_proof(
    context: &ProxyContext,
    data: &SingleNftContext,
) -> Proof<Signature, AccountId> {
    let data_to_sign = (
        SIGNED_MINT_SINGLE_NFT_CONTEXT,
        context.relayer.account_id(),
        &data.unique_external_ref,
        &data.royalties,
        data.t1_authority,
    );

    let signature = sign(&context.signer.key_pair(), &data_to_sign.encode());

    let proof = Proof::<Signature, AccountId> {
        signer: context.signer.account_id(),
        relayer: context.relayer.account_id(),
        signature,
    };

    return proof
}

pub fn single_nft_minted_events_emitted() -> bool {
    return single_nft_minted_events_count() > 0
}

pub fn single_nft_minted_events_count() -> usize {
    System::events()
        .into_iter()
        .map(|r| r.event)
        .filter_map(|e| if let RuntimeEvent::NftManager(inner) = e { Some(inner) } else { None })
        .count()
}
