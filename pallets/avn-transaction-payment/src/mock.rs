// This file is part of Aventus.
// Copyright (C) 2022 Aventus Network Services (UK) Ltd.

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use super::*;
use crate::{self as avn_transaction_payment};
use frame_support::{
   //assert_noop,
   assert_ok,
    dispatch::{DispatchClass},
    parameter_types,
    traits::{ConstU8},
    weights::{Weight, WeightToFee as WeightToFeeT},
    PalletId,
};

use frame_system::{self as system, limits};
use pallet_transaction_payment::CurrencyAdapter;

use sp_core::{sr25519, Pair, H256};
use sp_runtime::{
    testing::{Header},
    traits::{BlakeTwo256, IdentityLookup, Verify},
    Perbill, SaturatedConversion,
};


//use std::{cell::RefCell, sync::Arc};

use codec::Decode;

/// The signature type used by accounts/transactions.
pub type Signature = sr25519::Signature;
/// An identifier for an account on this system.
pub type AccountId = <Signature as Verify>::Signer;

// pub const AVT_TOKEN_CONTRACT: H160 = H160(hex!("dB1Cff52f66195f0a5Bd3db91137db98cfc54AE6"));
// pub const ONE_TOKEN: u128 = 1_000000_000000_000000u128;
// pub const AMOUNT_100_TOKEN: u128 = 100 * ONE_TOKEN;
// pub const AMOUNT_123_TOKEN: u128 = 123 * ONE_TOKEN;
pub const EXISTENTIAL_DEPOSIT: u64 = 0;
// pub const NON_AVT_TOKEN_ID: H160 = H160(hex!("1414141414141414141414141414141414141414"));
// pub const NON_AVT_TOKEN_ID_2: H160 = H160(hex!("2020202020202020202020202020202020202020"));

//const TOPIC_RECEIVER_INDEX: usize = 3;

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<TestRuntime>;
type Block = frame_system::mocking::MockBlock<TestRuntime>;

frame_support::construct_runtime!(
    pub enum TestRuntime where
        Block = Block,
        NodeBlock = Block,
        UncheckedExtrinsic = UncheckedExtrinsic,
    {
        System: frame_system::{Pallet, Call, Config, Storage, Event<T>},
        Balances: pallet_balances::{Pallet, Call, Storage, Config<T>, Event<T>},
        //AVN: pallet_avn::{Pallet, Storage},
        AvnTransactionPayment: avn_transaction_payment::{Pallet, Call, Storage, Event<T>},
        TransactionPayment: pallet_transaction_payment::{Pallet, Storage, Event<T>, Config},
    }
);

parameter_types! {
    pub const AvnTreasuryPotId: PalletId = PalletId(*b"Treasury");
    pub static TreasuryGrowthPercentage: Perbill = Perbill::from_percent(75);
}

impl avn_transaction_payment::Config for TestRuntime {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeCall = RuntimeCall;
    type Currency = Balances;
    type WeightInfo = ();
}

// impl avn::Config for TestRuntime {
//     type AuthorityId = UintAuthorityId;
//     type EthereumPublicKeyChecker = ();
//     type NewSessionHandler = ();
//     type DisabledValidatorChecker = ();
//     type FinalisedBlockChecker = ();
// }

// impl sp_runtime::BoundToRuntimeAppPublic for TestRuntime {
//     type Public = <mock::TestRuntime as avn::Config>::AuthorityId;
// }

pub const BASE_FEE: u64 = 12;

const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);
const MAX_BLOCK_WEIGHT: Weight = Weight::from_ref_time(1024).set_proof_size(u64::MAX);

parameter_types! {
    pub const BlockHashCount: u64 = 250;
    // Creating custom runtime block weights similar with substrate/frame/system/src/mock.rs
    pub BlockLength: limits::BlockLength = limits::BlockLength::max_with_normal_ratio(1024, NORMAL_DISPATCH_RATIO);
    pub RuntimeBlockWeights: limits::BlockWeights = limits::BlockWeights::builder()
        .base_block(Weight::from_ref_time(10))
        .for_class(DispatchClass::all(), |weights| {
            weights.base_extrinsic = Weight::from_ref_time(BASE_FEE);
        })
        .for_class(DispatchClass::Normal, |weights| {
            weights.max_total = Some(NORMAL_DISPATCH_RATIO * MAX_BLOCK_WEIGHT);
        })
        .for_class(DispatchClass::Operational, |weights| {
            weights.max_total = Some(MAX_BLOCK_WEIGHT);
            weights.reserved = Some(
                MAX_BLOCK_WEIGHT - NORMAL_DISPATCH_RATIO * MAX_BLOCK_WEIGHT
            );
    })
    .avg_block_initialization(Perbill::from_percent(0))
    .build_or_panic();
}

impl system::Config for TestRuntime {
    type BaseCallFilter = frame_support::traits::Everything;
    type BlockWeights = RuntimeBlockWeights;
    type BlockLength = BlockLength;
    type DbWeight = ();
    type RuntimeOrigin = RuntimeOrigin;
    type RuntimeCall = RuntimeCall;
    type Index = u64;
    type BlockNumber = u64;
    type Hash = H256;
    type Hashing = BlakeTwo256;
    type AccountId = AccountId;
    type Lookup = IdentityLookup<AccountId>;
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
    type MaxLocks = ();
    type Balance = u128;
    type DustRemoval = ();
    type RuntimeEvent = RuntimeEvent;
    type ExistentialDeposit = ExistentialDeposit;
    type AccountStore = System;
    type MaxReserves = ();
    type ReserveIdentifier = [u8; 8];
    type WeightInfo = ();
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

/// create a transaction info struct from weight. Handy to avoid building the whole struct.
// pub fn info_from_weight(w: Weight) -> DispatchInfo {
//     DispatchInfo { weight: w, ..Default::default() }
// }

#[derive(Clone)]
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

// pub fn genesis_collators() -> Vec<AccountId> {
//     return vec![TestAccount::new([1u8; 32]).account_id(), TestAccount::new([2u8; 32]).account_id()]
// }

pub struct ExtBuilder {
    storage: sp_runtime::Storage,
}

impl ExtBuilder {
    pub fn build_default() -> Self {
        let storage = frame_system::GenesisConfig::default().build_storage::<TestRuntime>().unwrap();
        Self { storage }
    }

    // pub fn with_genesis_config(mut self) -> Self {
    //     let _ = avn_transaction_payment::GenesisConfig::<TestRuntime> {
    //         _phantom: Default::default(),
    //         lower_account_id: H256::random(),
    //         avt_token_contract: AVT_TOKEN_CONTRACT,
    //     }
    //     .assimilate_storage(&mut self.storage);

    //     self
    // }

    // pub fn with_validators(mut self) -> Self {
    //     let genesis_accounts_ids = genesis_collators();
    //     frame_support::BasicExternalities::execute_with_storage(&mut self.storage, || {
    //         for ref k in genesis_accounts_ids.clone() {
    //             frame_system::Pallet::<TestRuntime>::inc_providers(k);
    //         }
    //     });

    //     self = self.with_balances();

    //     let _ = session::GenesisConfig::<TestRuntime> {
    //         keys: genesis_accounts_ids
    //             .clone()
    //             .into_iter()
    //             .enumerate()
    //             .map(|(i, v)| (v, v, UintAuthorityId((i as u32).into())))
    //             .collect(),
    //     }
    //     .assimilate_storage(&mut self.storage);

    //     let _ = parachain_staking::GenesisConfig::<TestRuntime> {
    //         candidates: genesis_accounts_ids.into_iter().map(|c| (c, 1000)).collect(),
    //         nominations: vec![],
    //         delay: 2,
    //         min_collator_stake: 10,
    //         min_total_nominator_stake: 5,
    //     }
    //     .assimilate_storage(&mut self.storage);

    //     self
    // }

    // pub fn with_balances(mut self) -> Self {
    //     let mut balances = vec![
    //         (account_id_with_100_avt(), AMOUNT_100_TOKEN),
    //         (account_id2_with_100_avt(), AMOUNT_100_TOKEN),
    //     ];
    //     balances.append(&mut genesis_collators().into_iter().map(|c| (c, 1000)).collect());

    //     let _ = pallet_balances::GenesisConfig::<TestRuntime> { balances }
    //         .assimilate_storage(&mut self.storage);
    //     self
    // }

    pub fn as_externality(self) -> sp_io::TestExternalities {


        let mut ext = sp_io::TestExternalities::from(self.storage);

        // Events do not get emitted on block 0, so we increment the block here
        ext.execute_with(|| System::set_block_number(1));
        ext
    }
}




// pub struct MockData {
//     // pub avt_token_lift_event: EthEvent,
//     // pub non_avt_token_lift_event: EthEvent,
//     // pub empty_data_lift_event: EthEvent,
//     pub receiver_account_id: <TestRuntime as system::Config>::AccountId,
//     //pub token_balance_123_tokens: <TestRuntime as Config>::TokenBalance,
// }

// impl MockData {
//     pub fn setup(_amount_to_lift: u128, _use_receiver_with_existing_amount: bool) -> Self {
//         // let lift_avt_token_event_topics =
//         //     Self::get_lifted_avt_token_topics(use_receiver_with_existing_amount);
//         // let lift_non_avt_token_event_topics =
//         //     Self::get_lifted_non_avt_token_topics(use_receiver_with_existing_amount);
//         let receiver_account_id = TestAccount::new([1u8; 32]).account_id();
//             //Self::get_receiver_account_id_from_topics(&lift_avt_token_event_topics);

//         // if use_receiver_with_existing_amount {
//         //     TokenManager::initialise_non_avt_tokens_to_account(
//         //         receiver_account_id,
//         //         AMOUNT_100_TOKEN,
//         //     );
//         // }

//         MockData {

//             receiver_account_id,
//             //token_balance_123_tokens: Self::get_token_balance(AMOUNT_123_TOKEN),
//         }
//     }
// }

// ============================= Signature handling ========================
// pub fn sign(signer: &sr25519::Pair, message_to_sign: &[u8]) -> Signature {
//     return Signature::from(signer.sign(message_to_sign))
// }

// pub fn get_account_id(signer: &sr25519::Pair) -> AccountId {
//     return AccountId::from(signer.public()).into_account()
// }
// ============================= Mock correctness tests ========================

#[test]
// Important - do not remove this test
fn avn_test_log_parsing_logic() {
    let mut ext = ExtBuilder::build_default().as_externality();

    ext.execute_with(|| {
        let sender = TestAccount::new([3u8; 32]).account_id();
        let config = AdjustmentInput::<TestRuntime> {
                fee_type: FeeType::FixedFee(FixedFeeConfig {
                    fee: 10
                }),
                adjustment_type: AdjustmentType::None,
        };
        assert_ok!(AvnTransactionPayment::set_known_sender(RuntimeOrigin::root(), sender, config));

            // We should never get here, but in case we do force test to fail
            assert!(true);

    });
}