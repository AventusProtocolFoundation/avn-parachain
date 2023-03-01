//! # Validators Manager pallet
// Copyright 2020 Artos Systems (UK) Ltd.

// validators manager pallet benchmarking.

#![cfg(feature = "runtime-benchmarks")]

use super::*;

use crate::Pallet as ValidatorManager;
use frame_benchmarking::{account, benchmarks, impl_benchmark_test_suite};
use frame_system::{EventRecord, Pallet as System, RawOrigin};
use hex::FromHex;
use hex_literal::hex;
use pallet_avn::{self as avn};
use pallet_parachain_staking::{Currency, Pallet as ParachainStaking};
use pallet_session::Pallet as Session;
use sp_core::ecdsa::Public;

fn setup_validators<T: Config>(
    number_of_validator_account_ids: u32,
) -> Vec<Validator<<T as pallet_avn::Config>::AuthorityId, T::AccountId>> {
    let mnemonic: &str =
        "basic anxiety marine match castle rival moral whisper insane away avoid bike";
    let mut validators: Vec<Validator<<T as pallet_avn::Config>::AuthorityId, T::AccountId>> =
        Vec::new();
    for i in 0..number_of_validator_account_ids {
        let account = account("dummy_validator", i, i);
        let key =
            <T as avn::Config>::AuthorityId::generate_pair(Some(mnemonic.as_bytes().to_vec()));
        validators.push(Validator::new(account, key));
    }

    // Setup sender account id and key
    let sender_index = validators.len() - (1 as usize);
    let sender: Validator<T::AuthorityId, T::AccountId> = validators[sender_index].clone();
    let mut account_bytes: [u8; 32] = [0u8; 32];
    account_bytes
        .copy_from_slice(&hex!("b41f90b123b66c18f0f869f3b9ae8a09d118419f8736240fcc7b8256517cc233"));
    let account_id = T::AccountId::decode(&mut &account_bytes.encode()[..]).unwrap();
    validators[sender_index] = Validator::new(account_id, sender.key);

    // Setup resigner account id and key
    let resigner: Validator<T::AuthorityId, T::AccountId> = validators[1].clone();
    let mut resigner_account_bytes: [u8; 32] = [0u8; 32];
    resigner_account_bytes
        .copy_from_slice(&hex!("1ed1aadead9704b693af012a9f24e1f00dc7e2a0b4eb99f9e0bc0c35a8d20223"));
    let resigner_account_id =
        T::AccountId::decode(&mut &resigner_account_bytes.encode()[..]).unwrap();
    validators[1] = Validator::new(resigner_account_id, resigner.key);

    // Setup validators in avn pallet
    avn::Validators::<T>::put(validators.clone());

    validators
        .iter()
        .enumerate()
        .for_each(|(i, v)| force_add_collator::<T>(&v.account_id, i as u64));

    return validators
}

fn setup_action_voting<T: Config>(
    validators: Vec<Validator<<T as pallet_avn::Config>::AuthorityId, T::AccountId>>,
) -> (
    Validator<T::AuthorityId, T::AccountId>,
    ActionId<T::AccountId>,
    ecdsa::Signature,
    <T::AuthorityId as RuntimeAppPublic>::Signature,
    u32,
) {
    let sender_index = validators.len() - (1 as usize);
    let sender: Validator<T::AuthorityId, T::AccountId> = validators[sender_index].clone();
    let action_account_id: T::AccountId = validators[1].account_id.clone();
    let ingress_counter: IngressCounter = 1;
    let action_id: ActionId<T::AccountId> = ActionId::new(action_account_id, ingress_counter);
    let approval_signature: ecdsa::Signature = ecdsa::Signature::from_slice(&hex!("2b01699be62c1aabaf0dd85f956567ac495d4293323ee1eb79d827d705ff86c80bdd4a26af6f50544af9510e0c21082b94ecb8a8d48d74ee4ebda6605a96d77901")).unwrap().into();
    let signature: <T::AuthorityId as RuntimeAppPublic>::Signature = generate_signature::<T>();
    let quorum = setup_voting_session::<T>(&action_id);

    let eth_public_key = Public::from_raw(
        <[u8; 33]>::from_hex("032717041a29cee520bb901c406e2399974d4ad58cd3ee830848bd78fa089f1335")
            .unwrap(),
    );

    EthereumPublicKeys::<T>::insert(eth_public_key.clone(), sender.account_id.clone());

    setup_action_data::<T>(
        sender.account_id.clone(),
        action_id.action_account_id.clone(),
        action_id.ingress_counter,
    );

    (sender, action_id, approval_signature, signature, quorum)
}

fn setup_action_data<T: Config>(
    sender: T::AccountId,
    action_account_id: T::AccountId,
    ingress_counter: IngressCounter,
) {
    let eth_transaction_id: TransactionId = 0;
    let candidate_tx = EthTransactionType::DeregisterValidator(DeregisterValidatorData::new(
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

    let quorum = calculate_two_third_quorum(AVN::<T>::validators().len() as u32);
    let voting_period_end =
        safe_add_block_numbers(<system::Pallet<T>>::block_number(), T::VotingPeriod::get());
    VotesRepository::<T>::insert(
        action_id,
        VotingSessionData::<T::AccountId, T::BlockNumber>::new(
            action_id.encode(),
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
            let approval_signature: ecdsa::Signature = generate_ecdsa_signature::<T>(i as u8);
            match is_approval {
                true => VotesRepository::<T>::mutate(action_id, |vote| {
                    vote.ayes.push(validators[i].account_id.clone());
                    vote.confirmations.push(approval_signature.clone());
                }),
                false => VotesRepository::<T>::mutate(action_id, |vote| {
                    vote.nays.push(validators[i].account_id.clone())
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

fn generate_ecdsa_signature<T: pallet_avn::Config>(msg: u8) -> ecdsa::Signature {
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

fn force_add_collator<T: Config>(collator_id: &T::AccountId, index: u64) {
    set_session_keys::<T>(collator_id, index);

    let eth_public_key: ecdsa::Public =
        ValidatorManager::<T>::compress_eth_public_key(H512::repeat_byte(index as u8));
    <T as pallet_parachain_staking::Config>::Currency::make_free_balance_be(
        &collator_id,
        ParachainStaking::<T>::min_collator_stake() * 2u32.into(),
    );
    ValidatorManager::<T>::add_collator(
        RawOrigin::Root.into(),
        collator_id.clone(),
        eth_public_key,
        None,
    )
    .unwrap();

    //Advance 2 session to add the collator to the session
    advance_session::<T>();
    advance_session::<T>();
}

benchmarks! {
    add_collator {
        let candidate = account("collator_cadidate", 1, 1);
        <T as pallet_parachain_staking::Config>::Currency::make_free_balance_be(&candidate, ParachainStaking::<T>::min_collator_stake() * 2u32.into());
        let eth_public_key: ecdsa::Public =
            ValidatorManager::<T>::compress_eth_public_key(H512::repeat_byte(6u8));
        set_session_keys::<T>(&candidate, 20u64);

        assert_eq!(false, pallet_parachain_staking::CandidateInfo::<T>::contains_key(&candidate));
    }: _(RawOrigin::Root, candidate.clone(), eth_public_key, None)
    verify {
        assert!(pallet_parachain_staking::CandidateInfo::<T>::contains_key(&candidate));
    }

    remove_validator {
        let v in (DEFAULT_MINIMUM_VALIDATORS_COUNT as u32 + 1) .. MAX_VALIDATOR_ACCOUNT_IDS;

        let validators = setup_validators::<T>(v);
        let caller = validators[(v - 1) as usize].account_id.clone();
    }: remove_validator(RawOrigin::Root, caller.clone())
    verify {
        assert_eq!(ValidatorAccountIds::<T>::get().unwrap().iter().position(|validator_account_id| *validator_account_id == caller), None);
        assert_last_event::<T>(Event::<T>::ValidatorDeregistered{ validator_id: caller.clone() }.into());
        assert_eq!(true, ValidatorActions::<T>::contains_key(caller, <TotalIngresses<T>>::get()));
    }

    approve_action_with_end_voting {
        let v in (DEFAULT_MINIMUM_VALIDATORS_COUNT as u32 + 1) .. MAX_VALIDATOR_ACCOUNT_IDS;

        let validators = setup_validators::<T>(v);
        let (sender, action_id, approval_signature, signature, quorum) = setup_action_voting::<T>(validators.clone());

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
        let v in (DEFAULT_MINIMUM_VALIDATORS_COUNT as u32 + 1) .. MAX_VALIDATOR_ACCOUNT_IDS;
        let validators = setup_validators::<T>(v);
        let (sender, action_id, approval_signature, signature, _) = setup_action_voting::<T>(validators);
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
        let v in (DEFAULT_MINIMUM_VALIDATORS_COUNT as u32 + 1) .. MAX_VALIDATOR_ACCOUNT_IDS;

        let validators = setup_validators::<T>(v);
        let (sender, action_id, _, signature, quorum) = setup_action_voting::<T>(validators.clone());

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

        let validators = setup_validators::<T>(v);
        let (sender, action_id, _, signature, _) = setup_action_voting::<T>(validators);
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
        let validators = setup_validators::<T>(number_of_validators);
        let (sender, action_id, _, signature, quorum) = setup_action_voting::<T>(validators.clone());

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
        let validators = setup_validators::<T>(number_of_validators);
        let (sender, action_id, _, signature, quorum) = setup_action_voting::<T>(validators.clone());

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
