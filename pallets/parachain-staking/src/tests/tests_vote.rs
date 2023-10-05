// Copyright 2022 Aventus Network Services (UK) Ltd.

#![cfg(test)]

use super::*;
use crate::{mock::*, GrowthId, Store, TransactionId, AVN};
use assert_matches::assert_matches;
use frame_support::{assert_noop, assert_ok};
use frame_system as system;
use pallet_avn::Error as AvNError;
use pallet_ethereum_transactions::{
    ethereum_transaction::{EthTransactionType, TriggerGrowthData},
    CandidateTransactionSubmitter,
};
use sp_core::{
    ecdsa,
    offchain::testing::{OffchainState, PendingRequest},
};
use sp_runtime::{
    testing::{TestSignature, UintAuthorityId},
    traits::BadOrigin,
    RuntimeAppPublic,
};
use system::RawOrigin;

const CURRENT_BLOCK_NUMBER: u64 = 10;
pub const VOTING_PERIOD_END: u64 = 12;
pub const QUORUM: u32 = 3;
pub const DEFAULT_INGRESS_COUNTER: IngressCounter = 100;
pub const APPROVE_ROOT: bool = true;
pub const REJECT_ROOT: bool = false;

#[derive(Clone)]
pub struct Context {
    pub current_block_number: u64,
    pub validator: Validator<UintAuthorityId, AccountId>,
    pub rewards_in_period: u128,
    pub total_stake_accumulated: u128,
    pub approval_signature: ecdsa::Signature,
    pub growth_id: GrowthId,
    pub tx_id: TransactionId,
    pub sr_signature: TestSignature,
}

pub const DEFAULT_VOTING_PERIOD: u64 = 2;

pub fn get_non_validator() -> Validator<UintAuthorityId, AccountId> {
    get_validator(10)
}

fn to_bytes(account_id: AccountId) -> [u8; 32] {
    let bytes = account_id.encode();
    let mut vector: [u8; 32] = Default::default();
    vector.copy_from_slice(&bytes[0..32]);
    return vector
}

pub fn setup_context() -> Context {
    ParachainStaking::set_voting_periods(DEFAULT_VOTING_PERIOD);

    let current_block_number = CURRENT_BLOCK_NUMBER;
    let growth_id = GrowthId::new(0u32, DEFAULT_INGRESS_COUNTER);
    let validator = get_validator(FIRST_VALIDATOR_INDEX);
    let approval_signature = ecdsa::Signature::try_from(&[1; 65][0..65]).unwrap();
    let rewards_in_period = 10u128;
    let total_stake_accumulated = 10u128;
    let sr_signature = validator.key.sign(&(growth_id).encode()).expect("Signature is signed");
    let tx_id = Test::reserve_transaction_id(&EthTransactionType::TriggerGrowth(
        TriggerGrowthData::new(10u128, 10u128, 0u32),
    ))
    .unwrap();

    Context {
        current_block_number,
        validator: validator.clone(),
        growth_id,
        rewards_in_period,
        total_stake_accumulated,
        approval_signature,
        sr_signature,
        tx_id,
    }
}

pub fn get_sign_url_param(growth_id: GrowthId) -> String {
    return ParachainStaking::convert_data_to_eth_compatible_encoding(&growth_id.period).unwrap()
}

pub fn mock_response_of_get_ecdsa_signature(
    state: &mut OffchainState,
    data_to_sign: String,
    response: Option<Vec<u8>>,
) {
    let mut url = "http://127.0.0.1:2020/eth/sign/".to_string();
    url.push_str(&data_to_sign);

    state.expect_request(PendingRequest {
        method: "GET".into(),
        uri: url.into(),
        response,
        sent: true,
        ..Default::default()
    });
}

pub fn set_vote_lock_with_expiry(block_number: BlockNumber, growth_id: &GrowthId) -> bool {
    OcwLock::set_lock_with_expiry(
        block_number,
        OcwOperationExpiration::Fast,
        vote::create_vote_lock_name::<Test>(growth_id),
    )
    .is_ok()
}

pub fn setup_blocks(context: &Context) {
    frame_system::Pallet::<Test>::set_block_number(context.current_block_number);
}

pub fn get_validator(index: u64) -> Validator<UintAuthorityId, AccountId> {
    Validator { account_id: TestAccount::new(index).account_id(), key: UintAuthorityId(index) }
}

fn setup_voting_for_growth_id(context: &Context, number_of_growths: Option<u32>) {
    setup_blocks(&context);

    for i in 0..number_of_growths.or_else(|| Some(1u32)).unwrap() {
        let mut growth_info = GrowthInfo::new(1u32);
        growth_info.total_staker_reward = context.rewards_in_period;
        growth_info.total_stake_accumulated = context.total_stake_accumulated;
        growth_info.tx_id = Some(context.tx_id);
        growth_info.added_by = Some(context.validator.account_id);

        ParachainStaking::insert_growth_data(i, growth_info);
    }

    let mut growth_info = GrowthInfo::new(1u32);
    growth_info.total_staker_reward = context.rewards_in_period;
    growth_info.total_stake_accumulated = context.total_stake_accumulated;
    growth_info.tx_id = Some(context.tx_id);
    growth_info.added_by = Some(context.validator.account_id);

    ParachainStaking::insert_growth_data(context.growth_id.period, growth_info);
    ParachainStaking::insert_pending_approval(&context.growth_id);
    ParachainStaking::register_growth_for_voting(&context.growth_id, QUORUM, VOTING_PERIOD_END);

    assert_eq!(ParachainStaking::get_vote(context.growth_id).ayes.is_empty(), true);
    assert_eq!(ParachainStaking::get_vote(context.growth_id).nays.is_empty(), true);
}

pub fn setup_approved_growth(context: Context) {
    setup_voting_for_growth_id(&context, None);

    let second_validator = get_validator(SECOND_VALIDATOR_INDEX);
    let third_validator = get_validator(THIRD_VALIDATOR_INDEX);
    ParachainStaking::record_approve_vote(&context.growth_id, context.validator.account_id);
    ParachainStaking::record_approve_vote(&context.growth_id, second_validator.account_id);
    ParachainStaking::record_approve_vote(&context.growth_id, third_validator.account_id);
}

pub fn vote_to_approve_growth(
    validator: &Validator<UintAuthorityId, AccountId>,
    context: &Context,
) -> bool {
    set_mock_recovered_account_id(to_bytes(validator.account_id));
    ParachainStaking::approve_growth(
        RawOrigin::None.into(),
        context.growth_id,
        validator.clone(),
        context.approval_signature.clone(),
        context.sr_signature.clone(),
    )
    .is_ok()
}

pub fn vote_to_reject_growth(
    validator: &Validator<UintAuthorityId, AccountId>,
    context: &Context,
) -> bool {
    ParachainStaking::reject_growth(
        RawOrigin::None.into(),
        context.growth_id,
        validator.clone(),
        context.sr_signature.clone(),
    )
    .is_ok()
}

pub fn get_signature_for_approve_cast_vote(
    signer: &Validator<UintAuthorityId, AccountId>,
    context: &[u8],
    growth_id: &GrowthId,
    eth_data_to_sign: &String,
    eth_signature: &ecdsa::Signature,
) -> TestSignature {
    signer
        .key
        .sign(
            &(
                context,
                growth_id.encode(),
                APPROVE_ROOT,
                eth_data_to_sign.encode(),
                eth_signature.encode(),
            )
                .encode(),
        )
        .expect("Signature is signed")
}

pub fn get_signature_for_reject_cast_vote(
    signer: &Validator<UintAuthorityId, AccountId>,
    context: &[u8],
    growth_id: &GrowthId,
) -> TestSignature {
    signer
        .key
        .sign(&(context, growth_id.encode(), REJECT_ROOT).encode())
        .expect("Signature is signed")
}

pub fn create_mock_identification_tuple(account_id: AccountId) -> (AccountId, AccountId) {
    return (account_id, account_id)
}

pub fn reported_offence(
    reporter: AccountId,
    validator_count: u32,
    offenders: Vec<ValidatorId>,
    offence_type: GrowthOffenceType,
) -> bool {
    let offences = ParachainStaking::get_offence_record();

    return offences.iter().any(|o| {
        offence_matches_criteria(
            o,
            vec![reporter],
            validator_count,
            offenders.iter().map(|v| create_mock_identification_tuple(*v)).collect(),
            offence_type.clone(),
        )
    })
}

pub fn reported_offence_of_type(offence_type: GrowthOffenceType) -> bool {
    let offences = ParachainStaking::get_offence_record();

    return offences.iter().any(|o| offence_is_of_type(o, offence_type.clone()))
}

fn offence_matches_criteria(
    this_report: &(Vec<ValidatorId>, Offence),
    these_reporters: Vec<ValidatorId>,
    this_count: u32,
    these_offenders: Vec<(ValidatorId, FullIdentification)>,
    this_type: GrowthOffenceType,
) -> bool {
    return matches!(
        this_report,
        (
            reporters,
            GrowthOffence {
                session_index: _,
                validator_set_count,
                offenders,
                offence_type}
        )
        if these_reporters == *reporters
        && this_count == *validator_set_count
        && these_offenders == *offenders
        && this_type == *offence_type
    )
}

fn offence_is_of_type(
    this_report: &(Vec<ValidatorId>, Offence),
    this_type: GrowthOffenceType,
) -> bool {
    return matches!(
        this_report,
        (
            _,
            GrowthOffence {
                session_index: _,
                validator_set_count: _,
                offenders: _,
                offence_type}
        )
        if this_type == *offence_type
    )
}

// TODO [TYPE: test][PRI: medium][JIRA: 321]
// Refactor the approve_growth and reject_growth tests so common codes can be shared
mod approve_growth {
    use super::*;

    mod succeeds {
        use super::*;

        #[test]
        fn when_one_validator_votes() {
            let (mut ext, _pool_state, _offchain_state) = ExtBuilder::default()
                .with_validators()
                .for_offchain_worker()
                .as_externality_with_state();

            ext.execute_with(|| {
                let context = setup_context();
                setup_voting_for_growth_id(&context, None);

                assert_eq!(
                    Result::Ok(()),
                    ParachainStaking::approve_growth(
                        RawOrigin::None.into(),
                        context.growth_id,
                        context.validator.clone(),
                        context.approval_signature.clone(),
                        context.sr_signature.clone()
                    )
                );

                assert_eq!(
                    ParachainStaking::get_vote(context.growth_id).ayes,
                    vec![context.validator.account_id]
                );
                assert_eq!(ParachainStaking::get_vote(context.growth_id).nays.is_empty(), true);

                assert!(System::events().iter().any(|a| a.event ==
                    mock::RuntimeEvent::ParachainStaking(Event::<Test>::VoteAdded {
                        voter: context.validator.account_id,
                        growth_id: context.growth_id,
                        agree_vote: true
                    })));
            });
        }

        #[test]
        fn when_two_validators_vote_differently() {
            let (mut ext, _pool_state, _offchain_state) = ExtBuilder::default()
                .with_validators()
                .for_offchain_worker()
                .as_externality_with_state();

            ext.execute_with(|| {
                let context = setup_context();

                setup_voting_for_growth_id(&context, None);
                assert!(vote_to_reject_growth(&context.validator, &context));
                let second_validator = get_validator(SECOND_VALIDATOR_INDEX);
                set_mock_recovered_account_id(to_bytes(second_validator.account_id));

                assert_eq!(
                    Result::Ok(()),
                    ParachainStaking::approve_growth(
                        RawOrigin::None.into(),
                        context.growth_id,
                        second_validator.clone(),
                        context.approval_signature.clone(),
                        context.sr_signature.clone()
                    )
                );

                assert_eq!(
                    ParachainStaking::get_vote(&(context.growth_id)).ayes,
                    vec![second_validator.account_id]
                );
                assert_eq!(
                    ParachainStaking::get_vote(&(context.growth_id)).nays,
                    vec![context.validator.account_id]
                );

                assert!(System::events().iter().any(|a| a.event ==
                    mock::RuntimeEvent::ParachainStaking(crate::Event::<Test>::VoteAdded {
                        voter: second_validator.account_id,
                        growth_id: context.growth_id,
                        agree_vote: true
                    })));
            });
        }

        #[test]
        fn when_two_validators_vote_the_same() {
            let (mut ext, _pool_state, _offchain_state) = ExtBuilder::default()
                .with_validators()
                .for_offchain_worker()
                .as_externality_with_state();

            ext.execute_with(|| {
                let context = setup_context();

                setup_voting_for_growth_id(&context, None);
                assert!(vote_to_approve_growth(&context.validator, &context));
                let second_validator = get_validator(SECOND_VALIDATOR_INDEX);
                set_mock_recovered_account_id(to_bytes(second_validator.account_id));

                assert_eq!(
                    Result::Ok(()),
                    ParachainStaking::approve_growth(
                        RawOrigin::None.into(),
                        context.growth_id,
                        second_validator.clone(),
                        context.approval_signature.clone(),
                        context.sr_signature.clone()
                    )
                );

                assert_eq!(
                    ParachainStaking::get_vote(context.growth_id).ayes,
                    vec![context.validator.account_id, second_validator.account_id]
                );
                assert_eq!(ParachainStaking::get_vote(context.growth_id).nays.is_empty(), true);

                assert!(System::events().iter().any(|a| a.event ==
                    mock::RuntimeEvent::ParachainStaking(crate::Event::<Test>::VoteAdded {
                        voter: second_validator.account_id,
                        growth_id: context.growth_id,
                        agree_vote: true
                    })));
            });
        }

        #[test]
        fn when_voting_is_not_finished() {
            let (mut ext, _pool_state, _offchain_state) = ExtBuilder::default()
                .with_validators()
                .for_offchain_worker()
                .as_externality_with_state();

            ext.execute_with(|| {
                let context = setup_context();

                setup_voting_for_growth_id(&context, None);
                let second_validator = get_validator(SECOND_VALIDATOR_INDEX);
                let third_validator = get_validator(THIRD_VALIDATOR_INDEX);
                assert!(vote_to_approve_growth(&context.validator, &context));
                assert!(vote_to_reject_growth(&second_validator, &context));

                set_mock_recovered_account_id(to_bytes(third_validator.account_id));
                assert_eq!(
                    Result::Ok(()),
                    ParachainStaking::approve_growth(
                        RawOrigin::None.into(),
                        context.growth_id,
                        third_validator.clone(),
                        context.approval_signature.clone(),
                        context.sr_signature.clone()
                    )
                );

                assert_eq!(
                    ParachainStaking::get_vote(context.growth_id).ayes,
                    vec![context.validator.account_id, third_validator.account_id]
                );
                assert_eq!(
                    ParachainStaking::get_vote(context.growth_id).nays,
                    vec![second_validator.account_id]
                );

                assert!(System::events().iter().any(|a| a.event ==
                    mock::RuntimeEvent::ParachainStaking(crate::Event::<Test>::VoteAdded {
                        voter: third_validator.account_id,
                        growth_id: context.growth_id,
                        agree_vote: true
                    })));
            });
        }
    }

    mod fails {
        use super::*;

        #[test]
        fn when_origin_is_signed() {
            let (mut ext, _pool_state, _offchain_state) = ExtBuilder::default()
                .with_validators()
                .for_offchain_worker()
                .as_externality_with_state();

            ext.execute_with(|| {
                let context = setup_context();

                setup_voting_for_growth_id(&context, None);

                assert_noop!(
                    ParachainStaking::approve_growth(
                        RuntimeOrigin::signed(context.validator.account_id),
                        context.growth_id,
                        context.validator,
                        context.approval_signature,
                        context.sr_signature
                    ),
                    BadOrigin
                );
            });
        }

        #[test]
        fn when_voter_is_invalid_validator() {
            let (mut ext, _pool_state, _offchain_state) = ExtBuilder::default()
                .with_validators()
                .for_offchain_worker()
                .as_externality_with_state();

            ext.execute_with(|| {
                let context = setup_context();

                setup_voting_for_growth_id(&context, None);
                set_mock_recovered_account_id(to_bytes(get_non_validator().account_id));

                let result = ParachainStaking::approve_growth(
                    RawOrigin::None.into(),
                    context.growth_id,
                    get_non_validator(),
                    context.approval_signature,
                    context.sr_signature,
                );

                // We can't use assert_noop here because we return an error after mutating storage
                assert_matches!(
                    result,
                    Err(e) if e == DispatchError::from(AvNError::<Test>::InvalidECDSASignature));
            });
        }

        #[test]
        fn when_growth_is_not_in_pending_approval() {
            let (mut ext, _pool_state, _offchain_state) = ExtBuilder::default()
                .with_validators()
                .for_offchain_worker()
                .as_externality_with_state();

            ext.execute_with(|| {
                let context = setup_context();

                setup_voting_for_growth_id(&context, None);
                ParachainStaking::remove_pending_approval(&context.growth_id.period);

                assert_noop!(
                    ParachainStaking::approve_growth(
                        RawOrigin::None.into(),
                        context.growth_id,
                        context.validator,
                        context.approval_signature,
                        context.sr_signature
                    ),
                    AvNError::<Test>::InvalidVote
                );
            });
        }

        #[test]
        fn when_growth_is_not_setup_for_voting() {
            let (mut ext, _pool_state, _offchain_state) = ExtBuilder::default()
                .with_validators()
                .for_offchain_worker()
                .as_externality_with_state();

            ext.execute_with(|| {
                let context = setup_context();

                ParachainStaking::register_growth_for_voting(
                    &context.growth_id,
                    QUORUM,
                    VOTING_PERIOD_END,
                );
                ParachainStaking::deregister_growth_for_voting(&context.growth_id);

                assert_noop!(
                    ParachainStaking::approve_growth(
                        RawOrigin::None.into(),
                        context.growth_id,
                        context.validator,
                        context.approval_signature,
                        context.sr_signature
                    ),
                    Error::<Test>::GrowthDataNotFound
                );
            });
        }

        #[test]
        fn when_voter_has_already_approved() {
            let (mut ext, _pool_state, _offchain_state) = ExtBuilder::default()
                .with_validators()
                .for_offchain_worker()
                .as_externality_with_state();

            ext.execute_with(|| {
                let context = setup_context();

                setup_voting_for_growth_id(&context, None);
                ParachainStaking::record_approve_vote(
                    &context.growth_id,
                    context.validator.account_id,
                );

                assert_noop!(
                    ParachainStaking::approve_growth(
                        RawOrigin::None.into(),
                        context.growth_id,
                        context.validator,
                        context.approval_signature,
                        context.sr_signature
                    ),
                    AvNError::<Test>::DuplicateVote
                );
            });
        }

        #[test]
        fn when_voter_has_already_rejected() {
            let (mut ext, _pool_state, _offchain_state) = ExtBuilder::default()
                .with_validators()
                .for_offchain_worker()
                .as_externality_with_state();

            ext.execute_with(|| {
                let context = setup_context();

                setup_voting_for_growth_id(&context, None);
                ParachainStaking::record_reject_vote(
                    &context.growth_id,
                    context.validator.account_id,
                );

                assert_noop!(
                    ParachainStaking::approve_growth(
                        RawOrigin::None.into(),
                        context.growth_id,
                        context.validator,
                        context.approval_signature,
                        context.sr_signature
                    ),
                    AvNError::<Test>::DuplicateVote
                );
            });
        }

        #[test]
        fn when_voting_is_finished() {
            let (mut ext, _pool_state, _offchain_state) = ExtBuilder::default()
                .with_validators()
                .for_offchain_worker()
                .as_externality_with_state();

            ext.execute_with(|| {
                let context = setup_context();

                setup_voting_for_growth_id(&context, None);
                let second_validator = get_validator(SECOND_VALIDATOR_INDEX);
                let third_validator = get_validator(THIRD_VALIDATOR_INDEX);
                let fourth_validator = get_validator(FOURTH_VALIDATOR_INDEX);
                assert!(vote_to_reject_growth(&context.validator, &context));
                assert!(vote_to_reject_growth(&second_validator, &context));
                assert!(vote_to_reject_growth(&third_validator, &context));

                set_mock_recovered_account_id(to_bytes(fourth_validator.account_id));
                assert_noop!(
                    ParachainStaking::approve_growth(
                        RawOrigin::None.into(),
                        context.growth_id,
                        fourth_validator.clone(),
                        context.approval_signature.clone(),
                        context.sr_signature.clone()
                    ),
                    AvNError::<Test>::InvalidVote
                );
            });
        }
    }
}

mod reject_growth {
    use super::*;

    mod succeeds {
        use super::*;

        #[test]
        fn when_one_validator_votes() {
            let (mut ext, _pool_state, _offchain_state) = ExtBuilder::default()
                .with_validators()
                .for_offchain_worker()
                .as_externality_with_state();

            ext.execute_with(|| {
                let context = setup_context();

                setup_voting_for_growth_id(&context, None);

                assert!(ParachainStaking::reject_growth(
                    RawOrigin::None.into(),
                    context.growth_id,
                    context.validator.clone(),
                    context.sr_signature.clone()
                )
                .is_ok());

                assert_eq!(ParachainStaking::get_vote(context.growth_id).ayes.is_empty(), true);
                assert_eq!(
                    ParachainStaking::get_vote(context.growth_id).nays,
                    vec![context.validator.account_id]
                );

                assert!(System::events().iter().any(|a| a.event ==
                    mock::RuntimeEvent::ParachainStaking(crate::Event::<Test>::VoteAdded {
                        voter: context.validator.account_id,
                        growth_id: context.growth_id,
                        agree_vote: false
                    })));
            });
        }

        #[test]
        fn when_two_validators_vote_differently() {
            let (mut ext, _pool_state, _offchain_state) = ExtBuilder::default()
                .with_validators()
                .for_offchain_worker()
                .as_externality_with_state();

            ext.execute_with(|| {
                let context = setup_context();

                setup_voting_for_growth_id(&context, None);
                assert!(vote_to_approve_growth(&context.validator, &context));
                let second_validator = get_validator(SECOND_VALIDATOR_INDEX);

                assert!(ParachainStaking::reject_growth(
                    RawOrigin::None.into(),
                    context.growth_id,
                    second_validator.clone(),
                    context.sr_signature.clone()
                )
                .is_ok());

                assert_eq!(
                    ParachainStaking::get_vote(context.growth_id).ayes,
                    vec![context.validator.account_id]
                );
                assert_eq!(
                    ParachainStaking::get_vote(&(context.growth_id)).nays,
                    vec![second_validator.account_id]
                );

                assert!(System::events().iter().any(|a| a.event ==
                    mock::RuntimeEvent::ParachainStaking(crate::Event::<Test>::VoteAdded {
                        voter: second_validator.account_id,
                        growth_id: context.growth_id,
                        agree_vote: false
                    })));
            });
        }

        #[test]
        fn when_two_validators_vote_the_same() {
            let (mut ext, _pool_state, _offchain_state) = ExtBuilder::default()
                .with_validators()
                .for_offchain_worker()
                .as_externality_with_state();

            ext.execute_with(|| {
                let context = setup_context();

                setup_voting_for_growth_id(&context, None);
                assert!(vote_to_reject_growth(&context.validator, &context));
                let second_validator = get_validator(SECOND_VALIDATOR_INDEX);

                assert!(ParachainStaking::reject_growth(
                    RawOrigin::None.into(),
                    context.growth_id,
                    second_validator.clone(),
                    context.sr_signature.clone()
                )
                .is_ok());

                assert_eq!(ParachainStaking::get_vote(context.growth_id).ayes.is_empty(), true);
                assert_eq!(
                    ParachainStaking::get_vote(context.growth_id).nays,
                    vec![context.validator.account_id, second_validator.account_id]
                );

                assert!(System::events().iter().any(|a| a.event ==
                    mock::RuntimeEvent::ParachainStaking(crate::Event::<Test>::VoteAdded {
                        voter: second_validator.account_id,
                        growth_id: context.growth_id,
                        agree_vote: false
                    })));
            });
        }

        #[test]
        fn when_voting_is_not_finished() {
            let (mut ext, _pool_state, _offchain_state) = ExtBuilder::default()
                .with_validators()
                .for_offchain_worker()
                .as_externality_with_state();

            ext.execute_with(|| {
                let context = setup_context();

                setup_voting_for_growth_id(&context, None);
                let second_validator = get_validator(SECOND_VALIDATOR_INDEX);
                let third_validator = get_validator(THIRD_VALIDATOR_INDEX);
                assert!(vote_to_approve_growth(&context.validator, &context));
                assert!(vote_to_reject_growth(&second_validator, &context));

                assert!(ParachainStaking::reject_growth(
                    RawOrigin::None.into(),
                    context.growth_id,
                    third_validator.clone(),
                    context.sr_signature.clone()
                )
                .is_ok());

                assert_eq!(
                    ParachainStaking::get_vote(context.growth_id).ayes,
                    vec![context.validator.account_id]
                );
                assert_eq!(
                    ParachainStaking::get_vote(context.growth_id).nays,
                    vec![second_validator.account_id, third_validator.account_id]
                );

                assert!(System::events().iter().any(|a| a.event ==
                    mock::RuntimeEvent::ParachainStaking(crate::Event::<Test>::VoteAdded {
                        voter: third_validator.account_id,
                        growth_id: context.growth_id,
                        agree_vote: false
                    })));
            });
        }
    }

    mod fails {
        use super::*;

        #[test]
        fn when_origin_is_signed() {
            let (mut ext, _pool_state, _offchain_state) = ExtBuilder::default()
                .with_validators()
                .for_offchain_worker()
                .as_externality_with_state();

            ext.execute_with(|| {
                let context = setup_context();

                setup_voting_for_growth_id(&context, None);

                assert_noop!(
                    ParachainStaking::reject_growth(
                        RuntimeOrigin::signed(context.validator.account_id),
                        context.growth_id,
                        context.validator,
                        context.sr_signature
                    ),
                    BadOrigin
                );
            });
        }

        #[test]
        fn when_voter_is_invalid_validator() {
            let (mut ext, _pool_state, _offchain_state) = ExtBuilder::default()
                .with_validators()
                .for_offchain_worker()
                .as_externality_with_state();

            ext.execute_with(|| {
                let context = setup_context();

                setup_voting_for_growth_id(&context, None);

                assert_noop!(
                    ParachainStaking::reject_growth(
                        RawOrigin::None.into(),
                        context.growth_id,
                        get_non_validator(),
                        context.sr_signature
                    ),
                    AvNError::<Test>::NotAValidator
                );
            });
        }

        #[test]
        fn when_growth_is_not_in_pending_approval() {
            let (mut ext, _pool_state, _offchain_state) = ExtBuilder::default()
                .with_validators()
                .for_offchain_worker()
                .as_externality_with_state();

            ext.execute_with(|| {
                let context = setup_context();

                setup_voting_for_growth_id(&context, None);
                ParachainStaking::remove_pending_approval(&context.growth_id.period);

                assert_noop!(
                    ParachainStaking::reject_growth(
                        RawOrigin::None.into(),
                        context.growth_id,
                        context.validator,
                        context.sr_signature
                    ),
                    AvNError::<Test>::InvalidVote
                );
            });
        }

        #[test]
        fn when_growth_is_not_setup_for_voting() {
            let (mut ext, _pool_state, _offchain_state) = ExtBuilder::default()
                .with_validators()
                .for_offchain_worker()
                .as_externality_with_state();

            ext.execute_with(|| {
                let context = setup_context();

                ParachainStaking::register_growth_for_voting(
                    &context.growth_id,
                    QUORUM,
                    VOTING_PERIOD_END,
                );
                ParachainStaking::deregister_growth_for_voting(&context.growth_id);

                assert_noop!(
                    ParachainStaking::reject_growth(
                        RawOrigin::None.into(),
                        context.growth_id,
                        context.validator,
                        context.sr_signature
                    ),
                    AvNError::<Test>::InvalidVote
                );
            });
        }

        #[test]
        fn when_voter_has_already_rejected() {
            let (mut ext, _pool_state, _offchain_state) = ExtBuilder::default()
                .with_validators()
                .for_offchain_worker()
                .as_externality_with_state();

            ext.execute_with(|| {
                let context = setup_context();

                setup_voting_for_growth_id(&context, None);
                ParachainStaking::record_reject_vote(
                    &context.growth_id,
                    context.validator.account_id,
                );

                assert_noop!(
                    ParachainStaking::reject_growth(
                        RawOrigin::None.into(),
                        context.growth_id,
                        context.validator,
                        context.sr_signature
                    ),
                    AvNError::<Test>::DuplicateVote
                );
            });
        }

        #[test]
        fn when_voter_has_already_approved() {
            let (mut ext, _pool_state, _offchain_state) = ExtBuilder::default()
                .with_validators()
                .for_offchain_worker()
                .as_externality_with_state();

            ext.execute_with(|| {
                let context = setup_context();

                setup_voting_for_growth_id(&context, None);
                ParachainStaking::record_approve_vote(
                    &context.growth_id,
                    context.validator.account_id,
                );

                assert_noop!(
                    ParachainStaking::reject_growth(
                        RawOrigin::None.into(),
                        context.growth_id,
                        context.validator,
                        context.sr_signature
                    ),
                    AvNError::<Test>::DuplicateVote
                );
            });
        }

        #[test]
        fn when_voting_is_finished() {
            let (mut ext, _pool_state, _offchain_state) = ExtBuilder::default()
                .with_validators()
                .for_offchain_worker()
                .as_externality_with_state();

            ext.execute_with(|| {
                let context = setup_context();

                setup_voting_for_growth_id(&context, None);
                let second_validator = get_validator(SECOND_VALIDATOR_INDEX);
                let third_validator = get_validator(THIRD_VALIDATOR_INDEX);
                let fourth_validator = get_validator(FOURTH_VALIDATOR_INDEX);
                assert!(vote_to_approve_growth(&context.validator, &context));
                assert!(vote_to_approve_growth(&second_validator, &context));
                assert!(vote_to_approve_growth(&third_validator, &context));

                assert_noop!(
                    ParachainStaking::reject_growth(
                        RawOrigin::None.into(),
                        context.growth_id,
                        fourth_validator.clone(),
                        context.sr_signature.clone()
                    ),
                    AvNError::<Test>::InvalidVote
                );
            });
        }
    }
}

mod cast_votes_if_required {
    use super::*;

    mod does_not_send_transactions {
        use super::*;

        #[test]
        #[ignore]
        fn when_setting_lock_with_expiry_has_error() {
            let (mut ext, pool_state, _offchain_state) = ExtBuilder::default()
                .with_validators()
                .for_offchain_worker()
                .as_externality_with_state();

            ext.execute_with(|| {
                let context = setup_context();

                setup_voting_for_growth_id(&context, None);

                assert!(set_vote_lock_with_expiry(
                    context.current_block_number,
                    &context.growth_id
                ));

                let second_validator = get_validator(SECOND_VALIDATOR_INDEX);
                cast_votes_if_required::<Test>(context.current_block_number, &second_validator);

                assert!(pool_state.read().transactions.is_empty());
            });
        }
    }

    #[test]
    fn sends_approve_vote_transaction_when_growth_is_valid() {
        let (mut ext, pool_state, offchain_state) = ExtBuilder::default()
            .with_validators()
            .for_offchain_worker()
            .as_externality_with_state();

        ext.execute_with(|| {
            let context = setup_context();

            setup_voting_for_growth_id(&context, Some(2u32));
            let second_validator = get_validator(SECOND_VALIDATOR_INDEX);

            let sign_url_param = get_sign_url_param(context.growth_id);
            mock_response_of_get_ecdsa_signature(
                &mut offchain_state.write(),
                sign_url_param.clone(),
                Some(hex::encode([1; 65].to_vec()).as_bytes().to_vec()),
            );

            cast_votes_if_required::<Test>(context.current_block_number, &second_validator);

            let tx = pool_state.write().transactions.pop().unwrap();
            assert!(pool_state.read().transactions.is_empty());
            let tx = Extrinsic::decode(&mut &*tx).unwrap();
            assert_eq!(tx.signature, None);

            assert_eq!(
                tx.call,
                mock::RuntimeCall::ParachainStaking(crate::Call::approve_growth {
                    growth_id: context.growth_id,
                    validator: second_validator.clone(),
                    approval_signature: context.approval_signature.clone(),
                    signature: get_signature_for_approve_cast_vote(
                        &second_validator,
                        CAST_VOTE_CONTEXT,
                        &context.growth_id,
                        &sign_url_param,
                        &context.approval_signature
                    )
                })
            );
        });
    }

    #[test]
    fn sends_reject_vote_transaction_when_growth_hash_is_invalid() {
        let (mut ext, pool_state, _offchain_state) = ExtBuilder::default()
            .with_validators()
            .for_offchain_worker()
            .as_externality_with_state();

        ext.execute_with(|| {
            let context = setup_context();

            setup_voting_for_growth_id(&context, None);
            let second_validator = get_validator(SECOND_VALIDATOR_INDEX);

            let bad_total_staker_reward = 0u128;
            let mut growth_data =
                ParachainStaking::try_get_growth_data(&context.growth_id.period).unwrap();
            growth_data.total_staker_reward = bad_total_staker_reward;
            ParachainStaking::insert_growth_data(context.growth_id.period, growth_data);

            cast_votes_if_required::<Test>(context.current_block_number, &second_validator);

            let tx = pool_state.write().transactions.pop().unwrap();
            assert!(pool_state.read().transactions.is_empty());
            let tx = Extrinsic::decode(&mut &*tx).unwrap();
            assert_eq!(tx.signature, None);

            assert_eq!(
                tx.call,
                mock::RuntimeCall::ParachainStaking(crate::Call::reject_growth {
                    growth_id: context.growth_id,
                    validator: second_validator.clone(),
                    signature: get_signature_for_reject_cast_vote(
                        &second_validator,
                        CAST_VOTE_CONTEXT,
                        &context.growth_id
                    )
                })
            );
        });
    }
}

mod end_voting_period {
    use super::*;

    mod succeeds {
        use super::*;

        #[test]
        fn when_a_vote_reached_quorum() {
            let (mut ext, _pool_state, _offchain_state) =
                ExtBuilder::default().for_offchain_worker().as_externality_with_state();

            ext.execute_with(|| {
                let context = setup_context();

                setup_approved_growth(context.clone());

                assert!(ParachainStaking::end_voting_period(
                    RawOrigin::None.into(),
                    context.growth_id,
                    context.validator.clone(),
                    context.sr_signature.clone(),
                )
                .is_ok());
                assert_eq!(
                    Some(true),
                    ParachainStaking::try_get_growth_data(&context.growth_id.period)
                        .unwrap()
                        .triggered
                );
                assert!(!<ParachainStaking as Store>::PendingApproval::contains_key(
                    &context.growth_id.period
                ));

                assert!(System::events().iter().any(|a| a.event ==
                    mock::RuntimeEvent::ParachainStaking(crate::Event::<Test>::VotingEnded {
                        growth_id: context.growth_id,
                        vote_approved: true
                    })));
            });
        }

        #[test]
        fn when_end_of_voting_period_passed() {
            let (mut ext, _pool_state, _offchain_state) =
                ExtBuilder::default().for_offchain_worker().as_externality_with_state();

            ext.execute_with(|| {
                let context = setup_context();

                setup_voting_for_growth_id(&context, None);
                System::set_block_number(50);

                assert!(ParachainStaking::end_voting_period(
                    RawOrigin::None.into(),
                    context.growth_id,
                    context.validator.clone(),
                    context.sr_signature.clone(),
                )
                .is_ok());
                assert_eq!(
                    None,
                    ParachainStaking::try_get_growth_data(&context.growth_id.period)
                        .unwrap()
                        .triggered
                );
                assert!(!<ParachainStaking as Store>::PendingApproval::contains_key(
                    &context.growth_id.period
                ));

                assert!(System::events().iter().any(|a| a.event ==
                    mock::RuntimeEvent::ParachainStaking(crate::Event::<Test>::VotingEnded {
                        growth_id: context.growth_id,
                        vote_approved: false
                    })));
            });
        }
    }

    mod fails {
        use super::*;

        #[test]
        fn when_origin_is_signed() {
            let (mut ext, _pool_state, _offchain_state) =
                ExtBuilder::default().for_offchain_worker().as_externality_with_state();

            ext.execute_with(|| {
                let context = setup_context();

                setup_approved_growth(context.clone());

                assert_noop!(
                    ParachainStaking::end_voting_period(
                        RuntimeOrigin::signed(context.validator.account_id),
                        context.growth_id,
                        context.validator.clone(),
                        context.sr_signature.clone(),
                    ),
                    BadOrigin
                );
            });
        }

        mod when_end_voting {
            use super::*;

            #[test]
            fn growth_is_not_setup_for_votes() {
                let (mut ext, _pool_state, _offchain_state) =
                    ExtBuilder::default().for_offchain_worker().as_externality_with_state();

                ext.execute_with(|| {
                    let context = setup_context();

                    setup_approved_growth(context.clone());
                    ParachainStaking::deregister_growth_for_voting(&context.growth_id);

                    assert_noop!(
                        ParachainStaking::end_voting_period(
                            RawOrigin::None.into(),
                            context.growth_id,
                            context.validator.clone(),
                            context.sr_signature.clone(),
                        ),
                        Error::<Test>::VotingSessionIsNotValid
                    );
                });
            }

            #[test]
            fn cannot_end_vote() {
                let (mut ext, _pool_state, _offchain_state) =
                    ExtBuilder::default().for_offchain_worker().as_externality_with_state();

                ext.execute_with(|| {
                    let context = setup_context();

                    setup_voting_for_growth_id(&context, None);

                    assert_noop!(
                        ParachainStaking::end_voting_period(
                            RawOrigin::None.into(),
                            context.growth_id,
                            context.validator.clone(),
                            context.sr_signature.clone(),
                        ),
                        Error::<Test>::ErrorEndingVotingPeriod
                    );
                });
            }

            #[test]
            fn submit_candidate_transaction_to_tier1_fails() {
                let (mut ext, _pool_state, _offchain_state) = ExtBuilder::default()
                    .with_validators()
                    .for_offchain_worker()
                    .as_externality_with_state();

                ext.execute_with(|| {
                    let context = setup_context();

                    setup_approved_growth(context.clone());

                    let mut growth_data = ParachainStaking::try_get_growth_data(
                        &GROWTH_PERIOD_THAT_CAUSES_SUBMISSION_TO_T1_ERROR,
                    )
                    .unwrap();
                    growth_data.total_staker_reward =
                        TOTAL_REWARD_IN_PERIOD_THAT_CAUSES_SUBMISSION_TO_T1_ERROR;
                    growth_data.total_stake_accumulated =
                        AVERAGE_STAKE_THAT_CAUSES_SUBMISSION_TO_T1_ERROR;
                    ParachainStaking::insert_growth_data(
                        GROWTH_PERIOD_THAT_CAUSES_SUBMISSION_TO_T1_ERROR,
                        growth_data,
                    );

                    assert_noop!(
                        ParachainStaking::end_voting_period(
                            RawOrigin::None.into(),
                            context.growth_id,
                            context.validator.clone(),
                            context.sr_signature.clone(),
                        ),
                        Error::<Test>::ErrorSubmitCandidateTxnToTier1
                    );
                });
            }
        }
    }

    mod offence_logic {
        use super::*;

        const TEST_VALIDATOR_COUNT: u64 = 5;

        // fn validator_indices() -> Vec<ValidatorId> {
        //     return (1..=TEST_VALIDATOR_COUNT).collect::<Vec<ValidatorId>>()
        // }

        mod when_growth_is_approved {
            use super::*;

            fn setup_approved_growth(
                context: &Context,
            ) -> (
                Vec<ValidatorId>, // ayes
                Vec<ValidatorId>, // nays
            ) {
                let aye_validator_1 = get_validator(1u64).account_id;
                let aye_validator_2 = get_validator(2u64).account_id;
                let aye_validator_3 = get_validator(3u64).account_id;
                let nay_validator_1 = get_validator(4u64).account_id;
                let nay_validator_2 = get_validator(5u64).account_id;
                assert_eq!(context.validator.account_id, aye_validator_1);

                ParachainStaking::record_approve_vote(&context.growth_id, aye_validator_1);
                ParachainStaking::record_reject_vote(&context.growth_id, nay_validator_1);
                ParachainStaking::record_approve_vote(&context.growth_id, aye_validator_2);
                ParachainStaking::record_reject_vote(&context.growth_id, nay_validator_2);
                ParachainStaking::record_approve_vote(&context.growth_id, aye_validator_3);

                return (
                    vec![aye_validator_1, aye_validator_2, aye_validator_3],
                    vec![nay_validator_1, nay_validator_2],
                )
            }

            #[test]
            fn reports_offence_for_nay_voters_only() {
                let (mut ext, _pool_state, _offchain_state) = ExtBuilder::default()
                    .with_validator_count(TEST_VALIDATOR_COUNT)
                    .for_offchain_worker()
                    .as_externality_with_state();
                ext.execute_with(|| {
                    let active_validators = AVN::<Test>::validators();
                    assert_eq!(active_validators.len(), TEST_VALIDATOR_COUNT as usize);

                    let context = setup_context();
                    setup_voting_for_growth_id(&context, None);
                    let (_ayes, nays) = setup_approved_growth(&context);

                    assert_ok!(ParachainStaking::end_voting_period(
                        RawOrigin::None.into(),
                        context.growth_id,
                        context.validator.clone(),
                        context.sr_signature.clone()
                    ));
                    assert_eq!(true, ParachainStaking::get_vote(context.growth_id).has_outcome());
                    assert_eq!(
                        Some(true),
                        ParachainStaking::try_get_growth_data(&context.growth_id.period)
                            .unwrap()
                            .triggered
                    );
                    assert_eq!(true, ParachainStaking::get_vote(context.growth_id).is_approved());

                    assert_eq!(
                        true,
                        reported_offence(
                            context.validator.account_id,
                            TEST_VALIDATOR_COUNT.try_into().unwrap(),
                            vec![nays[0], nays[1]],
                            GrowthOffenceType::RejectedValidGrowth
                        )
                    );

                    assert_eq!(
                        false,
                        reported_offence_of_type(GrowthOffenceType::ApprovedInvalidGrowth)
                    );
                });
            }
        }

        mod when_growth_is_rejected {
            use super::*;

            fn setup_rejected_growth(
                context: &Context,
            ) -> (
                Vec<ValidatorId>, // ayes
                Vec<ValidatorId>, // nays
            ) {
                let aye_validator_1 = get_validator(1u64).account_id;
                let aye_validator_2 = get_validator(2u64).account_id;
                let nay_validator_1 = get_validator(3u64).account_id;
                let nay_validator_2 = get_validator(4u64).account_id;
                let nay_validator_3 = get_validator(5u64).account_id;
                assert_eq!(context.validator.account_id, aye_validator_1);

                ParachainStaking::record_approve_vote(&context.growth_id, aye_validator_1);
                ParachainStaking::record_approve_vote(&context.growth_id, aye_validator_2);
                ParachainStaking::record_reject_vote(&context.growth_id, nay_validator_1);
                ParachainStaking::record_reject_vote(&context.growth_id, nay_validator_2);
                ParachainStaking::record_reject_vote(&context.growth_id, nay_validator_3);

                return (
                    vec![aye_validator_1, aye_validator_2],
                    vec![nay_validator_1, nay_validator_2, nay_validator_3],
                )
            }

            #[test]
            fn reports_offence_for_aye_voters_only() {
                let (mut ext, _pool_state, _offchain_state) = ExtBuilder::default()
                    .with_validator_count(TEST_VALIDATOR_COUNT)
                    .for_offchain_worker()
                    .as_externality_with_state();
                ext.execute_with(|| {
                    let active_validators = AVN::<Test>::validators();
                    assert_eq!(active_validators.len(), TEST_VALIDATOR_COUNT as usize);

                    let context = setup_context();
                    setup_voting_for_growth_id(&context, None);
                    let (ayes, _nays) = setup_rejected_growth(&context);

                    assert_ok!(ParachainStaking::end_voting_period(
                        RawOrigin::None.into(),
                        context.growth_id,
                        context.validator.clone(),
                        context.sr_signature.clone()
                    ));
                    assert_eq!(true, ParachainStaking::get_vote(context.growth_id).has_outcome());
                    assert_eq!(
                        None,
                        ParachainStaking::try_get_growth_data(&context.growth_id.period)
                            .unwrap()
                            .triggered
                    );
                    assert_eq!(false, ParachainStaking::get_vote(context.growth_id).is_approved());

                    assert_eq!(
                        true,
                        reported_offence(
                            context.validator.account_id,
                            TEST_VALIDATOR_COUNT.try_into().unwrap(),
                            vec![ayes[0], ayes[1]],
                            GrowthOffenceType::ApprovedInvalidGrowth
                        )
                    );
                    assert_eq!(
                        false,
                        reported_offence_of_type(GrowthOffenceType::RejectedValidGrowth)
                    );
                });
            }
        }

        mod when_vote_has_no_outcome {
            use super::*;

            fn setup_growth_without_outcome(
                context: &Context,
            ) -> (
                Vec<ValidatorId>, // ayes
                Vec<ValidatorId>, // nays
            ) {
                let aye_validator_1 = get_validator(1u64).account_id;
                let aye_validator_2 = get_validator(2u64).account_id;
                let nay_validator_1 = get_validator(3u64).account_id;
                let nay_validator_2 = get_validator(4u64).account_id;
                assert_eq!(context.validator.account_id, aye_validator_1);

                ParachainStaking::record_approve_vote(&context.growth_id, aye_validator_1);
                ParachainStaking::record_approve_vote(&context.growth_id, aye_validator_2);
                ParachainStaking::record_reject_vote(&context.growth_id, nay_validator_1);
                ParachainStaking::record_reject_vote(&context.growth_id, nay_validator_2);

                return (
                    vec![aye_validator_1, aye_validator_2],
                    vec![nay_validator_1, nay_validator_2],
                )
            }

            fn end_voting_without_outcome(context: &Context) {
                System::set_block_number(50);
                assert_ok!(ParachainStaking::end_voting_period(
                    RawOrigin::None.into(),
                    context.growth_id,
                    context.validator.clone(),
                    context.sr_signature.clone()
                ));
            }

            #[test]
            fn growth_is_not_approved() {
                let (mut ext, _pool_state, _offchain_state) = ExtBuilder::default()
                    .with_validator_count(TEST_VALIDATOR_COUNT)
                    .for_offchain_worker()
                    .as_externality_with_state();
                ext.execute_with(|| {
                    let active_validators = AVN::<Test>::validators();
                    assert_eq!(active_validators.len(), TEST_VALIDATOR_COUNT as usize);

                    let context = setup_context();
                    setup_voting_for_growth_id(&context, None);
                    let (_ayes, _nays) = setup_growth_without_outcome(&context);

                    end_voting_without_outcome(&context);

                    assert_eq!(
                        false,
                        ParachainStaking::get_vote(context.growth_id.clone()).has_outcome()
                    );
                    assert_eq!(
                        None,
                        ParachainStaking::try_get_growth_data(&context.growth_id.period)
                            .unwrap()
                            .triggered
                    );
                    assert_eq!(
                        false,
                        ParachainStaking::get_vote(context.growth_id.clone()).is_approved()
                    );
                });
            }

            #[test]
            fn does_not_report_rejected_valid_growth_offences() {
                let (mut ext, _pool_state, _offchain_state) = ExtBuilder::default()
                    .with_validator_count(TEST_VALIDATOR_COUNT)
                    .for_offchain_worker()
                    .as_externality_with_state();
                ext.execute_with(|| {
                    let active_validators = AVN::<Test>::validators();
                    assert_eq!(active_validators.len(), TEST_VALIDATOR_COUNT as usize);

                    let context = setup_context();
                    setup_voting_for_growth_id(&context, None);
                    let (_ayes, _nays) = setup_growth_without_outcome(&context);

                    end_voting_without_outcome(&context);

                    assert_eq!(
                        false,
                        reported_offence_of_type(GrowthOffenceType::RejectedValidGrowth)
                    );
                });
            }

            #[test]
            fn reports_approved_invalid_growth_offence() {
                let (mut ext, _pool_state, _offchain_state) = ExtBuilder::default()
                    .with_validator_count(TEST_VALIDATOR_COUNT)
                    .for_offchain_worker()
                    .as_externality_with_state();
                ext.execute_with(|| {
                    let active_validators = AVN::<Test>::validators();
                    assert_eq!(active_validators.len(), TEST_VALIDATOR_COUNT as usize);

                    let context = setup_context();
                    setup_voting_for_growth_id(&context, None);
                    let (ayes, _nays) = setup_growth_without_outcome(&context);

                    end_voting_without_outcome(&context);

                    assert_eq!(
                        true,
                        reported_offence(
                            context.validator.account_id,
                            TEST_VALIDATOR_COUNT.try_into().unwrap(),
                            vec![ayes[0], ayes[1]],
                            GrowthOffenceType::ApprovedInvalidGrowth
                        )
                    );
                });
            }
        }
    }
}
