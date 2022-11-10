use crate::{
    assert_eq_events, assert_eq_last_events, assert_event_emitted, assert_last_event,
    assert_tail_eq,
    mock::{
        roll_one_block, roll_to, roll_to_era_begin, roll_to_era_end, set_author, set_reward_pot,
        AccountId, Balances, Event as MetaEvent, ExtBuilder, Origin, ParachainStaking, Test,
        TestAccount, ErasPerGrowthPeriod
    },
    nomination_requests::{CancelledScheduledRequest, NominationAction, ScheduledRequest},
    AtStake, BalanceOf, Bond, CollatorStatus, EraIndex, Error, Event, GrowthInfo, NominationScheduledRequests, NominatorAdded,
    NominatorState, NominatorStatus, NOMINATOR_LOCK_ID,
};
use frame_support::{assert_noop, assert_ok};
use sp_runtime::{traits::Zero, DispatchError, ModuleError};

fn to_acc_id(id: u64) -> AccountId {
    return TestAccount::new(id).account_id()
}

fn roll_one_growth_period(current_era_index: EraIndex) {
    roll_to_era_begin((current_era_index + ErasPerGrowthPeriod::get()).into());
}

// #[test]
// fn initial_growth_state_is_ok() {
//     ExtBuilder::default().build().execute_with(|| {
//         // Growth period starts from 0
//         let growth_period_info = ParachainStaking::growth_period_info();
//         assert_eq!(growth_period_info.start_era_index, 0u32);
//         assert_eq!(growth_period_info.index, 0u32);

//         // The first growth is empty
//         let expected_growth: GrowthInfo<AccountId, BalanceOf<Test>> = GrowthInfo::new(1u32);
//         let initial_growth = ParachainStaking::growth(0);
//         assert_eq!(initial_growth.number_of_accumulations, expected_growth.number_of_accumulations);
//         assert_eq!(initial_growth.total_stake_accumulated, expected_growth.total_stake_accumulated);
//         assert_eq!(initial_growth.total_staker_reward, expected_growth.total_staker_reward);
//         assert_eq!(initial_growth.total_points, expected_growth.total_points);
//         assert_eq!(initial_growth.collator_scores, expected_growth.collator_scores);
//     });
// }


#[test]
fn growth_calculated_correctly() {
    let collator_1 = to_acc_id(1u64);
    let collator_2 = to_acc_id(2u64);
    let collator_3 = to_acc_id(3u64);
    ExtBuilder::default()
        .with_balances(vec![
            (collator_1, 100),
            (collator_2, 100),
        ])
        .with_candidates(vec![
            (collator_1, 10),
            (collator_2, 10),
        ])
        .build()
        .execute_with(|| {
            let current_era_index = ParachainStaking::era().current;
            println!("\ncurrent_era_index: {:?}\n{:?}", current_era_index, ParachainStaking::growth_period_info());

            set_author(current_era_index, collator_1, 5);
            set_author(current_era_index, collator_2, 5);

            roll_one_growth_period(current_era_index);
            println!("\ncurrent_era_index: {:?}\n", current_era_index);
            let growth_period_info = ParachainStaking::growth_period_info();
            assert_eq!(growth_period_info.start_era_index, current_era_index);
            assert_eq!(growth_period_info.index, 1u32);

            let current_era_index = ParachainStaking::era().current;
            set_author(current_era_index, collator_1, 5);
            set_author(current_era_index, collator_2, 5);

            roll_one_growth_period(current_era_index);
            println!("\ncurrent_era_index: {:?}\n", current_era_index);
            let growth_period_info = ParachainStaking::growth_period_info();
            assert_eq!(growth_period_info.start_era_index, current_era_index + 2);
            assert_eq!(growth_period_info.index, 2u32);

        });
}


// #[test]
// fn nahu() {
//     let collator_1 = to_acc_id(1u64);
//     let collator_2 = to_acc_id(2u64);
//     let collator_3 = to_acc_id(3u64);
//     ExtBuilder::default()
//         .with_balances(vec![
//             (collator_1, 100),
//             (collator_2, 100),
//             (collator_3, 100),
//         ])
//         .with_candidates(vec![
//             (collator_1, 10),
//             (collator_2, 10),
//             (collator_3, 10),
//         ])
//         .build()
//         .execute_with(|| {
//             roll_to(8);
//             // chooses top TotalSelectedCandidates (5), in order
//             let mut expected = vec![
//                 Event::CollatorChosen {
//                     era: 2,
//                     collator_account: account_id_5,
//                     total_exposed_amount: 10,
//                 },
//                 Event::CollatorChosen {
//                     era: 2,
//                     collator_account: account_id_3,
//                     total_exposed_amount: 20,
//                 },
//                 Event::CollatorChosen {
//                     era: 2,
//                     collator_account: account_id_4,
//                     total_exposed_amount: 20,
//                 },
//                 Event::CollatorChosen {
//                     era: 2,
//                     collator_account: account_id,
//                     total_exposed_amount: 50,
//                 },
//                 Event::CollatorChosen {
//                     era: 2,
//                     collator_account: account_id_2,
//                     total_exposed_amount: 40,
//                 },
//                 Event::NewEra {
//                     starting_block: 5,
//                     era: 2,
//                     selected_collators_number: 5,
//                     total_balance: 140,
//                 },
//             ];
//             assert_eq_events!(expected.clone());
//             // ~ set block author as 1 for all blocks this era
//             set_author(2, account_id, 100);
//             // We now payout from a central pot so we need to fund it
//             set_reward_pot(50);
//             roll_to(16);
//             // distribute total issuance to collator 1 and its nominators 6, 7, 19
//             let mut new = vec![
//                 Event::CollatorChosen {
//                     era: 3,
//                     collator_account: account_id_5,
//                     total_exposed_amount: 10,
//                 },
//                 Event::CollatorChosen {
//                     era: 3,
//                     collator_account: account_id_3,
//                     total_exposed_amount: 20,
//                 },
//                 Event::CollatorChosen {
//                     era: 3,
//                     collator_account: account_id_4,
//                     total_exposed_amount: 20,
//                 },
//                 Event::CollatorChosen {
//                     era: 3,
//                     collator_account: account_id,
//                     total_exposed_amount: 50,
//                 },
//                 Event::CollatorChosen {
//                     era: 3,
//                     collator_account: account_id_2,
//                     total_exposed_amount: 40,
//                 },
//                 Event::NewEra {
//                     starting_block: 10,
//                     era: 3,
//                     selected_collators_number: 5,
//                     total_balance: 140,
//                 },
//                 Event::CollatorChosen {
//                     era: 4,
//                     collator_account: account_id_5,
//                     total_exposed_amount: 10,
//                 },
//                 Event::CollatorChosen {
//                     era: 4,
//                     collator_account: account_id_3,
//                     total_exposed_amount: 20,
//                 },
//                 Event::CollatorChosen {
//                     era: 4,
//                     collator_account: account_id_4,
//                     total_exposed_amount: 20,
//                 },
//                 Event::CollatorChosen {
//                     era: 4,
//                     collator_account: account_id,
//                     total_exposed_amount: 50,
//                 },
//                 Event::CollatorChosen {
//                     era: 4,
//                     collator_account: account_id_2,
//                     total_exposed_amount: 40,
//                 },
//                 Event::NewEra {
//                     starting_block: 15,
//                     era: 4,
//                     selected_collators_number: 5,
//                     total_balance: 140,
//                 },
//                 Event::Rewarded {
//                     /*Explanation of how reward is computed:
//                         Total staked = 50
//                         Total reward to be paid = 50
//                         Collator stake = 20

//                         collator gets 40% ([collator stake] * 100 / [total staked]) of the [total reward] = 20 (40 * 50 / 100)
//                         Total 20
//                     */
//                     account: account_id,
//                     rewards: 20,
//                 },
//                 Event::Rewarded {
//                     /*Explanation of how reward is computed:
//                         Total staked = 50
//                         Total reward to be paid = 50
//                         Nominator stake = 10

//                         nominator gets 20% ([nominator stake] * 100 / [total staked]) of 50 ([total reward]) = 10
//                         Total 10
//                     */
//                     account: account_id_6,
//                     rewards: 10,
//                 },
//                 Event::Rewarded { account: account_id_7, rewards: 10 },
//                 Event::Rewarded { account: account_id_10, rewards: 10 },
//             ];
//             expected.append(&mut new);
//             assert_eq_events!(expected.clone());
//             // ~ set block author as 1 for all blocks this era
//             set_author(3, account_id, 100);
//             set_author(4, account_id, 100);
//             set_author(5, account_id, 100);
//             // 1. ensure nominators are paid for 2 eras after they leave
//             assert_noop!(
//                 ParachainStaking::schedule_leave_nominators(Origin::signed(to_acc_id(66))),
//                 Error::<Test>::NominatorDNE
//             );
//             assert_ok!(ParachainStaking::schedule_leave_nominators(Origin::signed(account_id_6)));
//             // fast forward to block in which nominator 6 exit executes. Doing it in 2 steps so we
//             // can reset the reward pot
//             set_reward_pot(55);
//             roll_to(20);

//             set_reward_pot(56);
//             roll_to(25);
//             assert_ok!(ParachainStaking::execute_leave_nominators(
//                 Origin::signed(account_id_6),
//                 account_id_6,
//                 10
//             ));
//             set_reward_pot(58);
//             roll_to(30);
//             let mut new2 = vec![
//                 Event::NominatorExitScheduled {
//                     era: 4,
//                     nominator: account_id_6,
//                     scheduled_exit: 6,
//                 },
//                 Event::CollatorChosen {
//                     era: 5,
//                     collator_account: account_id_5,
//                     total_exposed_amount: 10,
//                 },
//                 Event::CollatorChosen {
//                     era: 5,
//                     collator_account: account_id_3,
//                     total_exposed_amount: 20,
//                 },
//                 Event::CollatorChosen {
//                     era: 5,
//                     collator_account: account_id_4,
//                     total_exposed_amount: 20,
//                 },
//                 Event::CollatorChosen {
//                     era: 5,
//                     collator_account: account_id,
//                     total_exposed_amount: 50,
//                 },
//                 Event::CollatorChosen {
//                     era: 5,
//                     collator_account: account_id_2,
//                     total_exposed_amount: 40,
//                 },
//                 Event::NewEra {
//                     starting_block: 20,
//                     era: 5,
//                     selected_collators_number: 5,
//                     total_balance: 140,
//                 },
//                 Event::Rewarded { account: account_id, rewards: 22 },
//                 Event::Rewarded { account: account_id_6, rewards: 11 },
//                 Event::Rewarded { account: account_id_7, rewards: 11 },
//                 Event::Rewarded { account: account_id_10, rewards: 11 },
//                 Event::CollatorChosen {
//                     era: 6,
//                     collator_account: account_id_5,
//                     total_exposed_amount: 10,
//                 },
//                 Event::CollatorChosen {
//                     era: 6,
//                     collator_account: account_id_3,
//                     total_exposed_amount: 20,
//                 },
//                 Event::CollatorChosen {
//                     era: 6,
//                     collator_account: account_id_4,
//                     total_exposed_amount: 20,
//                 },
//                 Event::CollatorChosen {
//                     era: 6,
//                     collator_account: account_id,
//                     total_exposed_amount: 50,
//                 },
//                 Event::CollatorChosen {
//                     era: 6,
//                     collator_account: account_id_2,
//                     total_exposed_amount: 40,
//                 },
//                 Event::NewEra {
//                     starting_block: 25,
//                     era: 6,
//                     selected_collators_number: 5,
//                     total_balance: 140,
//                 },
//                 Event::Rewarded { account: account_id, rewards: 22 },
//                 Event::Rewarded { account: account_id_6, rewards: 11 },
//                 Event::Rewarded { account: account_id_7, rewards: 11 },
//                 Event::Rewarded { account: account_id_10, rewards: 11 },
//                 Event::NominatorLeftCandidate {
//                     nominator: account_id_6,
//                     candidate: account_id,
//                     unstaked_amount: 10,
//                     total_candidate_staked: 40,
//                 },
//                 Event::NominatorLeft { nominator: account_id_6, unstaked_amount: 10 },
//                 Event::CollatorChosen {
//                     era: 7,
//                     collator_account: account_id_5,
//                     total_exposed_amount: 10,
//                 },
//                 Event::CollatorChosen {
//                     era: 7,
//                     collator_account: account_id_3,
//                     total_exposed_amount: 20,
//                 },
//                 Event::CollatorChosen {
//                     era: 7,
//                     collator_account: account_id_4,
//                     total_exposed_amount: 20,
//                 },
//                 Event::CollatorChosen {
//                     era: 7,
//                     collator_account: account_id,
//                     total_exposed_amount: 40,
//                 },
//                 Event::CollatorChosen {
//                     era: 7,
//                     collator_account: account_id_2,
//                     total_exposed_amount: 40,
//                 },
//                 Event::NewEra {
//                     starting_block: 30,
//                     era: 7,
//                     selected_collators_number: 5,
//                     total_balance: 130,
//                 },
//                 Event::Rewarded { account: account_id, rewards: 29 },
//                 Event::Rewarded { account: account_id_7, rewards: 14 },
//                 Event::Rewarded { account: account_id_10, rewards: 14 },
//             ];
//             expected.append(&mut new2);
//             assert_eq_events!(expected.clone());
//             // 6 won't be paid for this era because they left already
//             set_author(6, account_id, 100);
//             set_reward_pot(61);
//             roll_to(35);
//             // keep paying 6
//             let mut new3 = vec![
//                 Event::CollatorChosen {
//                     era: 8,
//                     collator_account: account_id_5,
//                     total_exposed_amount: 10,
//                 },
//                 Event::CollatorChosen {
//                     era: 8,
//                     collator_account: account_id_3,
//                     total_exposed_amount: 20,
//                 },
//                 Event::CollatorChosen {
//                     era: 8,
//                     collator_account: account_id_4,
//                     total_exposed_amount: 20,
//                 },
//                 Event::CollatorChosen {
//                     era: 8,
//                     collator_account: account_id,
//                     total_exposed_amount: 40,
//                 },
//                 Event::CollatorChosen {
//                     era: 8,
//                     collator_account: account_id_2,
//                     total_exposed_amount: 40,
//                 },
//                 Event::NewEra {
//                     starting_block: 35,
//                     era: 8,
//                     selected_collators_number: 5,
//                     total_balance: 130,
//                 },
//                 Event::Rewarded { account: account_id, rewards: 30 },
//                 Event::Rewarded { account: account_id_7, rewards: 15 },
//                 Event::Rewarded { account: account_id_10, rewards: 15 },
//             ];
//             expected.append(&mut new3);
//             assert_eq_events!(expected.clone());
//             set_author(7, account_id, 100);
//             set_reward_pot(64);
//             roll_to(40);
//             // no more paying 6
//             let mut new4 = vec![
//                 Event::CollatorChosen {
//                     era: 9,
//                     collator_account: account_id_5,
//                     total_exposed_amount: 10,
//                 },
//                 Event::CollatorChosen {
//                     era: 9,
//                     collator_account: account_id_3,
//                     total_exposed_amount: 20,
//                 },
//                 Event::CollatorChosen {
//                     era: 9,
//                     collator_account: account_id_4,
//                     total_exposed_amount: 20,
//                 },
//                 Event::CollatorChosen {
//                     era: 9,
//                     collator_account: account_id,
//                     total_exposed_amount: 40,
//                 },
//                 Event::CollatorChosen {
//                     era: 9,
//                     collator_account: account_id_2,
//                     total_exposed_amount: 40,
//                 },
//                 Event::NewEra {
//                     starting_block: 40,
//                     era: 9,
//                     selected_collators_number: 5,
//                     total_balance: 130,
//                 },
//                 Event::Rewarded { account: account_id, rewards: 32 },
//                 Event::Rewarded { account: account_id_7, rewards: 16 },
//                 Event::Rewarded { account: account_id_10, rewards: 16 },
//             ];
//             expected.append(&mut new4);
//             assert_eq_events!(expected.clone());
//             set_author(8, account_id, 100);
//             assert_ok!(ParachainStaking::nominate(
//                 Origin::signed(to_acc_id(8)),
//                 account_id,
//                 10,
//                 10,
//                 10
//             ));
//             set_reward_pot(67);
//             roll_to(45);
//             // new nomination is not rewarded yet
//             let mut new5 = vec![
//                 Event::Nomination {
//                     nominator: to_acc_id(8),
//                     locked_amount: 10,
//                     candidate: account_id,
//                     nominator_position: NominatorAdded::AddedToTop { new_total: 50 },
//                 },
//                 Event::CollatorChosen {
//                     era: 10,
//                     collator_account: account_id_5,
//                     total_exposed_amount: 10,
//                 },
//                 Event::CollatorChosen {
//                     era: 10,
//                     collator_account: account_id_3,
//                     total_exposed_amount: 20,
//                 },
//                 Event::CollatorChosen {
//                     era: 10,
//                     collator_account: account_id_4,
//                     total_exposed_amount: 20,
//                 },
//                 Event::CollatorChosen {
//                     era: 10,
//                     collator_account: account_id,
//                     total_exposed_amount: 50,
//                 },
//                 Event::CollatorChosen {
//                     era: 10,
//                     collator_account: account_id_2,
//                     total_exposed_amount: 40,
//                 },
//                 Event::NewEra {
//                     starting_block: 45,
//                     era: 10,
//                     selected_collators_number: 5,
//                     total_balance: 140,
//                 },
//                 Event::Rewarded { account: account_id, rewards: 33 },
//                 Event::Rewarded { account: account_id_7, rewards: 17 },
//                 Event::Rewarded { account: account_id_10, rewards: 17 },
//             ];
//             expected.append(&mut new5);
//             assert_eq_events!(expected.clone());
//             set_author(9, account_id, 100);
//             set_author(10, account_id, 100);
//             set_reward_pot(70);
//             roll_to(50);
//             // new nomination is still not rewarded yet
//             let mut new6 = vec![
//                 Event::CollatorChosen {
//                     era: 11,
//                     collator_account: account_id_5,
//                     total_exposed_amount: 10,
//                 },
//                 Event::CollatorChosen {
//                     era: 11,
//                     collator_account: account_id_3,
//                     total_exposed_amount: 20,
//                 },
//                 Event::CollatorChosen {
//                     era: 11,
//                     collator_account: account_id_4,
//                     total_exposed_amount: 20,
//                 },
//                 Event::CollatorChosen {
//                     era: 11,
//                     collator_account: account_id,
//                     total_exposed_amount: 50,
//                 },
//                 Event::CollatorChosen {
//                     era: 11,
//                     collator_account: account_id_2,
//                     total_exposed_amount: 40,
//                 },
//                 Event::NewEra {
//                     starting_block: 50,
//                     era: 11,
//                     selected_collators_number: 5,
//                     total_balance: 140,
//                 },
//                 Event::Rewarded { account: account_id, rewards: 35 },
//                 Event::Rewarded { account: account_id_7, rewards: 17 },
//                 Event::Rewarded { account: account_id_10, rewards: 17 },
//             ];
//             expected.append(&mut new6);
//             assert_eq_events!(expected.clone());
//             set_reward_pot(75);
//             roll_to(55);
//             // new nomination is rewarded, 2 eras after joining (`RewardPaymentDelay` is 2)
//             let mut new7 = vec![
//                 Event::CollatorChosen {
//                     era: 12,
//                     collator_account: account_id_5,
//                     total_exposed_amount: 10,
//                 },
//                 Event::CollatorChosen {
//                     era: 12,
//                     collator_account: account_id_3,
//                     total_exposed_amount: 20,
//                 },
//                 Event::CollatorChosen {
//                     era: 12,
//                     collator_account: account_id_4,
//                     total_exposed_amount: 20,
//                 },
//                 Event::CollatorChosen {
//                     era: 12,
//                     collator_account: account_id,
//                     total_exposed_amount: 50,
//                 },
//                 Event::CollatorChosen {
//                     era: 12,
//                     collator_account: account_id_2,
//                     total_exposed_amount: 40,
//                 },
//                 Event::NewEra {
//                     starting_block: 55,
//                     era: 12,
//                     selected_collators_number: 5,
//                     total_balance: 140,
//                 },
//                 Event::Rewarded { account: account_id, rewards: 30 },
//                 Event::Rewarded { account: account_id_7, rewards: 15 },
//                 Event::Rewarded { account: account_id_10, rewards: 15 },
//                 Event::Rewarded { account: to_acc_id(8), rewards: 15 },
//             ];
//             expected.append(&mut new7);
//             assert_eq_events!(expected);
//         });
// }