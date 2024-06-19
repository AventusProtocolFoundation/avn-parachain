// #[cfg(test)]
// mod tests {
//     use super::*;
//     use frame_support::{assert_ok, assert_err};
//     use frame_system::RawOrigin;
//     use sp_runtime::traits::Zero;

//     #[test]
//     fn test_vote_function_delegating() {
//         // Setup
//         let mut ext = sp_io::TestExternalities::default();
//         ext.execute_with(|| {
//             // Initialize pallet
//             let alice = 1;
//             let poll_index = 0;
//             let vote = AccountVote::Standard(100);
//             let class = 1;
//             let delegations = 50;

//             // Alice delegates her votes
//             pallet_conviction_voting::VotingFor::<Test, ()>::insert(alice, &class, &pallet_conviction_voting::Voting::Casting {
//                 votes: vec![(poll_index, vote.clone())],
//                 delegations,
//                 ..Default::default()
//             });

//             // Call vote function
//             assert_ok!(Pallet::<Test>::vote(RawOrigin::Signed(alice).into(), poll_index, vote.clone()));

//             // Check if vote is updated correctly
//             let voting = pallet_conviction_voting::VotingFor::<Test, ()>::get(alice, &class).unwrap();
//             if let pallet_conviction_voting::Voting::Casting(pallet_conviction_voting::Casting { votes, .. }) = voting {
//                 assert_eq!(votes[0].1, vote);
//             } else {
//                 panic!("Unexpected voting state");
//             }
//         });
//     }

//     #[test]
//     fn test_vote_function_not_delegating() {
//         // Setup
//         let mut ext = sp_io::TestExternalities::default();
//         ext.execute_with(|| {
//             // Initialize pallet
//             let alice = 1;
//             let poll_index = 0;
//             let vote = AccountVote::Standard(100);

//             // Alice is not delegating her votes
//             pallet_conviction_voting::VotingFor::<Test, ()>::insert(alice, &1, &pallet_conviction_voting::Voting::NotDelegating);

//             // Call vote function
//             assert_err!(Pallet::<Test>::vote(RawOrigin::Signed(alice).into(), poll_index, vote.clone()), Error::<Test>::AlreadyDelegating);
//         });
//     }

//     #[test]
//     fn test_vote_function_max_votes_reached() {
//         // Setup
//         let mut ext = sp_io::TestExternalities::default();
//         ext.execute_with(|| {
//             // Initialize pallet
//             let alice = 1;
//             let poll_index = 0;
//             let vote = AccountVote::Standard(100);
//             let class = 1;
//             let delegations = 50;

//             // Alice has already voted for the poll
//             pallet_conviction_voting::VotingFor::<Test, ()>::insert(alice, &class, &pallet_conviction_voting::Voting::Casting {
//                 votes: vec![(poll_index, vote.clone())],
//                 delegations,
//                 ..Default::default()
//             });

//             // Call vote function with the same poll index
//             assert_err!(Pallet::<Test>::vote(RawOrigin::Signed(alice).into(), poll_index, vote.clone()), Error::<Test>::MaxVotesReached);
//         });
//     }

//     #[test]
//     fn test_vote_function_not_ongoing_poll() {
//         // Setup
//         let mut ext = sp_io::TestExternalities::default();
//         ext.execute_with(|| {
//             // Initialize pallet
//             let alice = 1;
//             let poll_index = 0;
//             let vote = AccountVote::Standard(100);
//             let class = 1;
//             let delegations = 50;

//             // Poll is not ongoing
//             <T as VotingConfig>::Polls::insert(poll_index, PollStatus::Finished(Default::default()));

//             // Call vote function
//             assert_err!(Pallet::<Test>::vote(RawOrigin::Signed(alice).into(), poll_index, vote.clone()), Error::<Test>::NotOngoing);
//         });
//     }
// }