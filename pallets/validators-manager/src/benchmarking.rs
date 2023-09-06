//! # Validators Manager pallet
// Copyright 2020 Artos Systems (UK) Ltd.

// validators manager pallet benchmarking.

#![cfg(feature = "runtime-benchmarks")]

use super::*;

use crate::Pallet as ValidatorManager;
use frame_benchmarking::{account, benchmarks, impl_benchmark_test_suite};
use frame_system::{EventRecord, Pallet as System, RawOrigin};
use hex_literal::hex;
use pallet_avn::{self as avn};
use pallet_parachain_staking::{Currency, Pallet as ParachainStaking};
use pallet_session::Pallet as Session;
use secp256k1::{PublicKey, SecretKey};
use sp_avn_common::eth_key_actions::decompress_eth_public_key;
use sp_core::{ecdsa::Public, H512};
use sp_runtime::WeakBoundedVec;

// Resigner keys derived from [6u8; 32] private key
const RESIGNING_COLLATOR_PUBLIC_KEY_BYTES: [u8; 32] =
    hex!["ea3021db7da7831e0d5ed7e60a8102d2d721bcca88adb03ee992f4dec3baee3e"];
const RESIGNING_COLLATOR_ETHEREUM_PUBLIC_KEY: [u8; 33] =
    hex!["03f006a18d5653c4edf5391ff23a61f03ff83d237e880ee61187fa9f379a028e0a"];

// Vote sender keys derived from [7u8; 32] private key
const VOTING_COLLATOR_PUBLIC_KEY_BYTES: [u8; 32] =
    hex!["7c0f469d3bd340bae718203fa30ca071a5e37c751e891dbded837b213d45d91d"];
const VOTING_COLLATOR_ETHEREUM_PUBLIC_KEY: [u8; 33] =
    hex!["02989c0b76cb563971fdc9bef31ec06c3560f3249d6ee9e5d83c57625596e05f6f"];

const NEW_COLLATOR_ETHEREUM_PUBLIC_KEY: [u8; 33] =
    hex!["03f171af36531200540b2badee5ed581b0a51f4e4a1a995025e149b9721b050074"];

const MINIMUM_ADDITIONAL_BENCHMARKS_VALIDATORS: usize = 2;

fn generate_resigning_collator_account_details<T: Config>(
) -> (T::AccountId, <T as pallet_avn::Config>::AuthorityId, Public) {
    let authority_id =
        <T as avn::Config>::AuthorityId::generate_pair(Some("//avn_resigner".as_bytes().to_vec()));
    let eth_public_key = Public::from_raw(RESIGNING_COLLATOR_ETHEREUM_PUBLIC_KEY);
    let account_id =
        T::AccountId::decode(&mut RESIGNING_COLLATOR_PUBLIC_KEY_BYTES.as_slice()).unwrap();

    (account_id, authority_id, eth_public_key)
}

fn generate_sender_collator_account_details<T: Config>(
) -> (T::AccountId, <T as pallet_avn::Config>::AuthorityId, Public) {
    let authority_id =
        <T as avn::Config>::AuthorityId::generate_pair(Some("//avn_sender".as_bytes().to_vec()));
    let eth_public_key = Public::from_raw(VOTING_COLLATOR_ETHEREUM_PUBLIC_KEY);
    let account_id =
        T::AccountId::decode(&mut VOTING_COLLATOR_PUBLIC_KEY_BYTES.as_slice()).unwrap();

    (account_id, authority_id, eth_public_key)
}

// Add additional collators, on top of genesis configuration
fn setup_additional_validators<T: Config>(number_of_additional_validators: u32) {
    assert!(number_of_additional_validators >= MINIMUM_ADDITIONAL_BENCHMARKS_VALIDATORS as u32);

    let mut avn_validators: Vec<Validator<<T as pallet_avn::Config>::AuthorityId, T::AccountId>> =
        Vec::new();

    let mut validators: Vec<(T::AccountId, Public)> = Vec::new();
    let vote_sender_index = number_of_additional_validators - (1 as u32);

    for i in 0..number_of_additional_validators {
        let (account, avn_authority_id, eth_key) = match i {
            0 => generate_resigning_collator_account_details::<T>(),
            i if i == vote_sender_index => generate_sender_collator_account_details::<T>(),
            _ => (
                account("dummy_validator", i, i),
                <T as avn::Config>::AuthorityId::generate_pair(None),
                generate_collator_eth_public_key_from_seed::<T>(i as u64),
            ),
        };

        avn_validators.push(Validator::new(account.clone(), avn_authority_id));
        validators.push((account, eth_key));
    }

    // Setup validators in avn pallet
    let mut new_avn_validators = avn::Validators::<T>::get();
    // new_avn_validators.append(&mut avn_validators.clone());
    let combined_avn_validators: Vec<_> =
        new_avn_validators.iter().chain(avn_validators.iter()).cloned().collect();
    avn::Validators::<T>::put(WeakBoundedVec::force_from(
        combined_avn_validators,
        Some("Too many validators for session"),
    ));

    validators.iter().enumerate().for_each(|(i, (account_id, eth_public_key))| {
        force_add_collator::<T>(&account_id, i as u64, &eth_public_key)
    });
}

fn setup_action_voting<T: Config>() -> (
    Validator<T::AuthorityId, T::AccountId>,
    ActionId<T::AccountId>,
    ecdsa::Signature,
    <T::AuthorityId as RuntimeAppPublic>::Signature,
    u32,
) {
    let (vote_sender_account, vote_sender_avn_authority_id, _) =
        generate_sender_collator_account_details::<T>();
    let (action_account_id, _, _) = generate_resigning_collator_account_details::<T>();

    let ingress_counter: IngressCounter = 1;

    let action_id: ActionId<T::AccountId> =
        ActionId::new(action_account_id.clone(), ingress_counter);

    let signature: <T::AuthorityId as RuntimeAppPublic>::Signature = generate_signature::<T>();
    let quorum = setup_voting_session::<T>(&action_id);
    // signed by sender private key [7u8; 32]
    let approval_signature: ecdsa::Signature = ecdsa::Signature::from_slice(&hex!("120898af9793fcbed12bf40c01a5adff6f310410276de344db50019f15c05d2c27254d2c14aa61692f00b398b6db582930764429a5f6fe37e371479d523e11571b")).unwrap().into();

    setup_resignation_action_data::<T>(vote_sender_account.clone(), action_id.ingress_counter);
    // Action has been setup
    (
        Validator::new(vote_sender_account, vote_sender_avn_authority_id),
        action_id,
        approval_signature,
        signature,
        quorum,
    )
}

fn setup_resignation_action_data<T: Config>(sender: T::AccountId, ingress_counter: IngressCounter) {
    let (action_account_id, _, t1_eth_public_key) =
        generate_resigning_collator_account_details::<T>();

    let eth_transaction_id: TransactionId = 0;
    let decompressed_eth_public_key = decompress_eth_public_key(t1_eth_public_key)
        .map_err(|_| Error::<T>::InvalidPublicKey)
        .unwrap();
    let candidate_tx = EthTransactionType::DeregisterCollator(DeregisterCollatorData::new(
        decompressed_eth_public_key,
        <T as Config>::AccountToBytesConvert::into_bytes(&action_account_id),
    ));

    #[cfg(test)]
    T::CandidateTransactionSubmitter::reserve_transaction_id(&candidate_tx.clone()).unwrap();
    #[cfg(not(test))]
    T::CandidateTransactionSubmitter::set_transaction_id(&candidate_tx.clone(), eth_transaction_id);

    ValidatorActions::<T>::insert(
        action_account_id,
        ingress_counter,
        ValidatorsActionData::new(
            ValidatorsActionStatus::AwaitingConfirmation,
            sender,
            eth_transaction_id,
            ValidatorsActionType::Resignation,
            candidate_tx,
        ),
    )
}

fn setup_voting_session<T: Config>(action_id: &ActionId<T::AccountId>) -> u32 {
    PendingApprovals::<T>::insert(action_id.action_account_id.clone(), action_id.ingress_counter);

    let quorum = calculate_one_third_quorum(AVN::<T>::validators().len() as u32);
    let voting_period_end =
        safe_add_block_numbers(<system::Pallet<T>>::block_number(), T::VotingPeriod::get());
    VotesRepository::<T>::insert(
        action_id,
        VotingSessionData::<T::AccountId, T::BlockNumber>::new(
            action_id.session_id(),
            quorum,
            voting_period_end.expect("already checked"),
            0u32.into(),
        ),
    );

    return quorum
}

fn setup_approval_votes<T: Config>(
    validators: &Vec<Validator<<T as pallet_avn::Config>::AuthorityId, T::AccountId>>,
    sender: &Validator<<T as pallet_avn::Config>::AuthorityId, T::AccountId>,
    number_of_votes: u32,
    action_id: &ActionId<T::AccountId>,
) {
    setup_votes::<T>(validators, sender, number_of_votes, action_id, true);
}

fn setup_reject_votes<T: Config>(
    validators: &Vec<Validator<<T as pallet_avn::Config>::AuthorityId, T::AccountId>>,
    sender: &Validator<<T as pallet_avn::Config>::AuthorityId, T::AccountId>,
    number_of_votes: u32,
    action_id: &ActionId<T::AccountId>,
) {
    setup_votes::<T>(validators, sender, number_of_votes, action_id, false);
}

fn setup_votes<T: Config>(
    validators: &Vec<Validator<<T as pallet_avn::Config>::AuthorityId, T::AccountId>>,
    sender: &Validator<<T as pallet_avn::Config>::AuthorityId, T::AccountId>,
    number_of_votes: u32,
    action_id: &ActionId<T::AccountId>,
    is_approval: bool,
) {
    for i in 0..validators.len() {
        if i < (number_of_votes as usize) && validators[i].account_id != sender.account_id.clone() {
            let approval_signature: ecdsa::Signature = generate_mock_ecdsa_signature::<T>(i as u8);
            match is_approval {
                true => VotesRepository::<T>::mutate(action_id, |vote| {
                    vote.ayes
                        .try_push(validators[i].account_id.clone())
                        .expect("Failed to add mock aye vote");
                    vote.confirmations
                        .try_push(approval_signature.clone())
                        .expect("Failed to add mock confirmation vote");
                }),
                false => VotesRepository::<T>::mutate(action_id, |vote| {
                    vote.nays
                        .try_push(validators[i].account_id.clone())
                        .expect("Failed to add mock nay vote");
                }),
            }
        }
    }
}

fn generate_signature<T: pallet_avn::Config>(
) -> <<T as avn::Config>::AuthorityId as RuntimeAppPublic>::Signature {
    let encoded_data = 0.encode();
    let authority_id = T::AuthorityId::generate_pair(None);
    let signature = authority_id.sign(&encoded_data).expect("able to make signature");
    return signature
}

fn generate_mock_ecdsa_signature<T: pallet_avn::Config>(msg: u8) -> ecdsa::Signature {
    let signature_bytes: [u8; 65] = [msg; 65];
    return ecdsa::Signature::from_slice(&signature_bytes).unwrap().into()
}

fn assert_last_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
    assert_last_nth_event::<T>(generic_event, 1);
}

fn assert_last_nth_event<T: Config>(generic_event: <T as Config>::RuntimeEvent, n: u32) {
    let events = frame_system::Pallet::<T>::events();
    let system_event: <T as frame_system::Config>::RuntimeEvent = generic_event.into();
    // Compare to the last event record
    let EventRecord { event, .. } = &events[events.len().saturating_sub(n as usize)];
    assert_eq!(event, &system_event);
}

fn advance_session<T: Config>() {
    use frame_support::traits::{OnFinalize, OnInitialize};

    let now = System::<T>::block_number().max(1u32.into());
    pallet_parachain_staking::ForceNewEra::<T>::put(true);

    System::<T>::on_finalize(System::<T>::block_number());
    System::<T>::set_block_number(now + 1u32.into());
    System::<T>::on_initialize(System::<T>::block_number());
    Session::<T>::on_initialize(System::<T>::block_number());
    ParachainStaking::<T>::on_initialize(System::<T>::block_number());
}

fn set_session_keys<T: Config>(collator_id: &T::AccountId, index: u64) {
    use rand::{RngCore, SeedableRng};

    frame_system::Pallet::<T>::inc_providers(collator_id);

    let keys = {
        let mut keys = [0u8; 128];
        // We keep the keys for the first validator as 0x00000...
        let mut rng = rand::rngs::StdRng::seed_from_u64(index);
        rng.fill_bytes(&mut keys);
        keys
    };

    let keys: T::Keys = Decode::decode(&mut &keys[..]).unwrap();

    pallet_session::Pallet::<T>::set_keys(
        RawOrigin::<T::AccountId>::Signed(collator_id.clone()).into(),
        keys,
        Vec::new(),
    )
    .unwrap();
}

fn generate_collator_eth_public_key_from_seed<T: Config>(seed: u64) -> Public {
    use rand::SeedableRng;
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    let secret_key = SecretKey::random(&mut rng);
    let public_key = PublicKey::from_secret_key(&secret_key);

    return ValidatorManager::<T>::compress_eth_public_key(H512::from_slice(
        &public_key.serialize()[1..],
    ))
}

fn force_add_collator<T: Config>(collator_id: &T::AccountId, index: u64, eth_public_key: &Public) {
    set_session_keys::<T>(collator_id, index);
    <T as pallet_parachain_staking::Config>::Currency::make_free_balance_be(
        &collator_id,
        ParachainStaking::<T>::min_collator_stake() * 2u32.into(),
    );
    ValidatorManager::<T>::add_collator(
        RawOrigin::Root.into(),
        collator_id.clone(),
        eth_public_key.clone(),
        None,
    )
    .unwrap();

    //Advance 2 session to add the collator to the session
    advance_session::<T>();
    advance_session::<T>();
}

benchmarks! {
    add_collator {
        let candidate = account("collator_candidate", 1, 1);
        <T as pallet_parachain_staking::Config>::Currency::make_free_balance_be(&candidate, ParachainStaking::<T>::min_collator_stake() * 2u32.into());
        let eth_public_key: ecdsa::Public = Public::from_raw(NEW_COLLATOR_ETHEREUM_PUBLIC_KEY);
        set_session_keys::<T>(&candidate, 20u64);

        assert_eq!(false, pallet_parachain_staking::CandidateInfo::<T>::contains_key(&candidate));
    }: _(RawOrigin::Root, candidate.clone(), eth_public_key, None)
    verify {
        assert!(pallet_parachain_staking::CandidateInfo::<T>::contains_key(&candidate));
    }

    remove_validator {
        let v in (MINIMUM_ADDITIONAL_BENCHMARKS_VALIDATORS as u32 + 1) .. MAX_VALIDATOR_ACCOUNT_IDS;

        setup_additional_validators::<T>(v);
        let (caller_account, caller_id, _) = generate_sender_collator_account_details::<T>();
        let caller = Validator::new(caller_account.clone(), caller_id.clone());

    }: remove_validator(RawOrigin::Root, caller_account.clone())
    verify {
        assert_eq!(ValidatorAccountIds::<T>::get().unwrap().iter().position(|validator_account_id| *validator_account_id == caller_account), None);
        assert_last_event::<T>(Event::<T>::ValidatorDeregistered{ validator_id: caller_account.clone() }.into());
        assert_eq!(true, ValidatorActions::<T>::contains_key(caller_account, <TotalIngresses<T>>::get()));
    }

    approve_action_with_end_voting {
        let v in (MINIMUM_ADDITIONAL_BENCHMARKS_VALIDATORS as u32 + 1) .. MAX_VALIDATOR_ACCOUNT_IDS;
        setup_additional_validators::<T>(v);
        let (sender, action_id, approval_signature, signature, quorum) = setup_action_voting::<T>();
        // Setup votes more than quorum to trigger end voting period
        let number_of_votes = quorum;
        setup_approval_votes::<T>(&AVN::<T>::validators(), &sender, number_of_votes, &action_id);
    }: approve_validator_action(RawOrigin::None, action_id.clone(), sender.clone(), approval_signature.clone(), signature)
    verify {
        // Approve vote is added
        assert_eq!(true, VotesRepository::<T>::get(action_id.clone()).ayes.contains(&sender.account_id.clone()));
        assert_eq!(true, VotesRepository::<T>::get(action_id.clone()).confirmations.contains(&approval_signature));

        // Voting period is ended
        assert_eq!((ValidatorActions::<T>::get(&action_id.action_account_id.clone(), action_id.ingress_counter)).unwrap().status, ValidatorsActionStatus::Actioned);
        assert_eq!(false, PendingApprovals::<T>::contains_key(&action_id.action_account_id.clone()));

        // Events are emitted
        assert_last_nth_event::<T>(
            Event::<T>::VotingEnded {
                action_id: action_id.clone(),
                vote_approved: (Box::new(ValidatorManagementVotingSession::<T>::new(&action_id.clone())) as Box<dyn VotingSessionManager<T::AccountId, T::BlockNumber>>).state()?.is_approved()
            }.into(),
            2
        );
        assert_last_event::<T>(Event::<T>::VoteAdded{ voter_id: sender.account_id, action_id: action_id.clone(), approve: APPROVE_VOTE }.into());
    }

    approve_action_without_end_voting {
        let v in (MINIMUM_ADDITIONAL_BENCHMARKS_VALIDATORS as u32 + 1) .. MAX_VALIDATOR_ACCOUNT_IDS;
        setup_additional_validators::<T>(v);
        let (sender, action_id, approval_signature, signature, _) = setup_action_voting::<T>();
    }: approve_validator_action(RawOrigin::None, action_id.clone(), sender.clone(), approval_signature.clone(), signature)
    verify {
        // Approve vote is added
        assert_eq!(true, VotesRepository::<T>::get(action_id.clone()).ayes.contains(&sender.account_id.clone()));
        assert_eq!(true, VotesRepository::<T>::get(action_id.clone()).confirmations.contains(&approval_signature));

        // Voting period is not ended
        assert_eq!(ValidatorActions::<T>::get(&action_id.action_account_id.clone(), action_id.ingress_counter).unwrap().status, ValidatorsActionStatus::AwaitingConfirmation);
        assert_eq!(true, PendingApprovals::<T>::contains_key(&action_id.action_account_id.clone()));

        // Event is emitted
        assert_last_event::<T>(Event::<T>::VoteAdded{ voter_id: sender.account_id, action_id: action_id.clone(), approve: APPROVE_VOTE }.into());
    }

    reject_action_with_end_voting {
        let v in (MINIMUM_ADDITIONAL_BENCHMARKS_VALIDATORS as u32 + 1) .. MAX_VALIDATOR_ACCOUNT_IDS;

        setup_additional_validators::<T>(v);
        let (sender, action_id, _, signature, quorum) = setup_action_voting::<T>();

        // Setup votes more than quorum to trigger end voting period
        let number_of_votes = quorum;
        setup_reject_votes::<T>(&AVN::<T>::validators(), &sender, number_of_votes, &action_id);
    }: reject_validator_action(RawOrigin::None, action_id.clone(), sender.clone(), signature)
    verify {
        // Reject vote is added
        assert_eq!(true, VotesRepository::<T>::get(action_id.clone()).nays.contains(&sender.account_id.clone()));

        // Voting period is ended, but deregistration is not actioned
        assert_eq!(
            ValidatorActions::<T>::get(
                &action_id.action_account_id.clone(),
                action_id.ingress_counter
            ).unwrap().status,
            ValidatorsActionStatus::AwaitingConfirmation);
        assert_eq!(false, PendingApprovals::<T>::contains_key(&action_id.action_account_id.clone()));

        // Events are emitted
        assert_last_nth_event::<T>(
            Event::<T>::VotingEnded {
                action_id: action_id.clone(),
                vote_approved: (Box::new(ValidatorManagementVotingSession::<T>::new(&action_id.clone())) as Box<dyn VotingSessionManager<T::AccountId, T::BlockNumber>>).state()?.is_approved()
            }.into(),
            2
        );
        assert_last_event::<T>(Event::<T>::VoteAdded{ voter_id: sender.account_id, action_id: action_id.clone(), approve: REJECT_VOTE }.into());
    }

    reject_action_without_end_voting {
        let v in (DEFAULT_MINIMUM_VALIDATORS_COUNT as u32 + 1) .. MAX_VALIDATOR_ACCOUNT_IDS;

        setup_additional_validators::<T>(v);
        let (sender, action_id, _, signature, _) = setup_action_voting::<T>();
    }: reject_validator_action(RawOrigin::None, action_id.clone(), sender.clone(), signature)
    verify {
        // Reject vote is added
        assert_eq!(true, VotesRepository::<T>::get(action_id.clone()).nays.contains(&sender.account_id.clone()));

        // Voting period is not ended
        assert_eq!(
            ValidatorActions::<T>::get(
                &action_id.action_account_id.clone(),
                action_id.ingress_counter
            ).unwrap().status,
            ValidatorsActionStatus::AwaitingConfirmation
        );
        assert_eq!(true, PendingApprovals::<T>::contains_key(&action_id.action_account_id.clone()));

        // Event is emitted
        assert_last_event::<T>(Event::<T>::VoteAdded{ voter_id: sender.account_id, action_id: action_id.clone(), approve: REJECT_VOTE }.into());
    }

    end_voting_period_with_rejected_valid_actions {
        let o in 1 .. MAX_OFFENDERS; // maximum num of offenders need to be less than one third of minimum validators so the benchmark won't panic

        let number_of_validators = MAX_VALIDATOR_ACCOUNT_IDS;
        setup_additional_validators::<T>(number_of_validators);
        let (sender, action_id, _, signature, quorum) = setup_action_voting::<T>();

        let all_collators = AVN::<T>::validators();

        // Setup votes more than quorum to trigger end voting period
        let number_of_approval_votes = quorum;
        setup_approval_votes::<T>(&all_collators, &sender, number_of_approval_votes + 1, &action_id);

        // setup offenders votes
        let (_, offenders) = all_collators.split_at(quorum as usize);
        let number_of_reject_votes = o;
        setup_reject_votes::<T>(&offenders.to_vec(), &sender, number_of_reject_votes, &action_id);
    }: end_voting_period(RawOrigin::None, action_id.clone(), sender.clone(), signature)
    verify {
        // Voting period is ended, and deregistration is actioned
        assert_eq!(
            ValidatorActions::<T>::get(
                &action_id.action_account_id.clone(),
                action_id.ingress_counter
            ).unwrap().status,
            ValidatorsActionStatus::Actioned);
        assert_eq!(false, PendingApprovals::<T>::contains_key(&action_id.action_account_id.clone()));

        // Events are emitted
        assert_last_event::<T>(Event::<T>::VotingEnded {
            action_id: action_id.clone(),
            vote_approved: (Box::new(ValidatorManagementVotingSession::<T>::new(&action_id.clone())) as Box<dyn VotingSessionManager<T::AccountId, T::BlockNumber>>).state()?.is_approved()}.into()
        );
    }

    end_voting_period_with_approved_invalid_actions {
        let o in 1 .. MAX_OFFENDERS; // maximum of offenders need to be less one third of minimum validators so the benchmark won't panic

        let number_of_validators = MAX_VALIDATOR_ACCOUNT_IDS;
        setup_additional_validators::<T>(number_of_validators);
        let (sender, action_id, _, signature, quorum) = setup_action_voting::<T>();

        let all_collators = AVN::<T>::validators();

        // Setup votes more than quorum to trigger end voting period
        let number_of_reject_votes = quorum;
        setup_reject_votes::<T>(&all_collators, &sender, number_of_reject_votes + 1, &action_id);

        // setup offenders votes
        let (_, offenders) = all_collators.split_at(quorum as usize);
        let number_of_approval_votes = o;
        setup_approval_votes::<T>(&offenders.to_vec(), &sender, number_of_approval_votes, &action_id);
    }: end_voting_period(RawOrigin::None, action_id.clone(), sender.clone(), signature)
    verify {
        // Voting period is ended, but deregistration is not actioned
        assert_eq!(
            ValidatorActions::<T>::get(
                &action_id.action_account_id.clone(),
                action_id.ingress_counter
            ).unwrap().status,
            ValidatorsActionStatus::AwaitingConfirmation);
        assert_eq!(false, PendingApprovals::<T>::contains_key(&action_id.action_account_id.clone()));

        // Events are emitted
        assert_last_event::<T>(Event::<T>::VotingEnded {
            action_id: action_id.clone(),
            vote_approved: (Box::new(ValidatorManagementVotingSession::<T>::new(&action_id.clone())) as Box<dyn VotingSessionManager<T::AccountId, T::BlockNumber>>).state()?.is_approved()
        }.into());
    }
}

impl_benchmark_test_suite!(
    Pallet,
    crate::mock::ExtBuilder::build_default().with_validators().as_externality(),
    crate::mock::TestRuntime,
);
