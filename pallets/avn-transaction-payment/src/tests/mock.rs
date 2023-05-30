use crate as pallet_avn_transaction_payment;
use codec::{Decode, Encode};
use frame_support::{
    traits::{ConstU16, ConstU64},
    weights::{Weight, WeightToFee as WeightToFeeT},
};
use sp_core::{sr25519, Pair, H256};
use sp_runtime::{
    testing::Header,
    traits::{BlakeTwo256, IdentityLookup, Verify},
    SaturatedConversion,
};

use pallet_transaction_payment::CurrencyAdapter;

use super::*;
use frame_support::{parameter_types, traits::ConstU8};

pub type AccountId = <Signature as Verify>::Signer;
pub type Signature = sr25519::Signature;

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<TestRuntime>;
type Block = frame_system::mocking::MockBlock<TestRuntime>;

pub const EXISTENTIAL_DEPOSIT: u64 = 0;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
    pub enum TestRuntime where
        Block = Block,
        NodeBlock = Block,
        UncheckedExtrinsic = UncheckedExtrinsic,
    {
        // System: frame_system,
        System: frame_system::{Pallet, Call, Config, Storage, Event<T>},
        Balances: pallet_balances::{Pallet, Call, Storage, Event<T>},
        TransactionPayment: pallet_transaction_payment::{Pallet, Storage, Event<T>, Config},
        AvnTransactionPayment: pallet_avn_transaction_payment::{Pallet, Call, Storage, Event<T>},
    }
);

impl frame_system::Config for TestRuntime {
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
    type BlockHashCount = ConstU64<250>;
    type Version = ();
    type PalletInfo = PalletInfo;
    type AccountData = pallet_balances::AccountData<u128>;
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type SystemWeightInfo = ();
    type SS58Prefix = ConstU16<42>;
    type OnSetCode = ();
    type MaxConsumers = frame_support::traits::ConstU32<16>;
}

impl pallet_avn_transaction_payment::Config for TestRuntime {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeCall = RuntimeCall;
    type Currency = Balances;
    type WeightInfo = pallet_avn_transaction_payment::default_weights::SubstrateWeight<TestRuntime>;
}

impl WeightToFeeT for WeightToFee {
    type Balance = u128;

    fn weight_to_fee(weight: &Weight) -> Self::Balance {
        Self::Balance::saturated_from(weight.ref_time())
            .saturating_mul(WEIGHT_TO_FEE.with(|v| *v.borrow()))
    }
}

impl WeightToFeeT for TransactionByteFee {
    type Balance = u128;

    fn weight_to_fee(weight: &Weight) -> Self::Balance {
        Self::Balance::saturated_from(weight.ref_time())
            .saturating_mul(TRANSACTION_BYTE_FEE.with(|v| *v.borrow()))
    }
}

parameter_types! {
    pub static WeightToFee: u128 = 1u128;
    pub static TransactionByteFee: u128 = 0u128;
}

impl pallet_transaction_payment::Config for TestRuntime {
    type RuntimeEvent = RuntimeEvent;
    type OnChargeTransaction = CurrencyAdapter<Balances, ()>;
    type LengthToFee = TransactionByteFee;
    type WeightToFee = WeightToFee;
    type FeeMultiplierUpdate = ();
    type OperationalFeeMultiplier = ConstU8<5>;
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

pub struct TestAccount {
    pub seed: [u8; 32],
}

impl TestAccount {
    pub fn new(id: u64) -> Self {
        TestAccount { seed: Self::into_32_bytes(&id) }
    }

    pub fn account_id(&self) -> AccountId {
        return AccountId::decode(&mut self.key_pair().public().to_vec().as_slice()).unwrap()
    }

    pub fn key_pair(&self) -> sr25519::Pair {
        return sr25519::Pair::from_seed(&self.seed)
    }

    fn into_32_bytes(account: &u64) -> [u8; 32] {
        let mut bytes = account.encode();
        let mut bytes32: Vec<u8> = vec![0; 32 - bytes.len()];
        bytes32.append(&mut bytes);
        let mut data: [u8; 32] = Default::default();
        data.copy_from_slice(&bytes32[0..32]);
        data
    }
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
    frame_system::GenesisConfig::default()
        .build_storage::<TestRuntime>()
        .unwrap()
        .into()
}
