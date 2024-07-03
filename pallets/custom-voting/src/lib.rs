#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::traits::{Currency, Polling};
pub use pallet::*;
pub use pallet_conviction_voting::{Config as VotingConfig, TallyOf};
pub mod default_weights;
use frame_support::{dispatch::GetDispatchInfo, traits::IsSubType};
use sp_runtime::traits::{Dispatchable, Hash, IdentifyAccount, Verify};
use sp_std::{boxed::Box, vec::Vec};
use sp_core::ecdsa::Signature;


#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[cfg(test)]
#[path = "tests/mock.rs"]
mod mock;

#[cfg(test)]
#[path = "tests/tests.rs"]
mod tests;

pub type PollIndexOf<T, I = ()> = <<T as VotingConfig<I>>::Polls as Polling<TallyOf<T, I>>>::Index;
pub type BalanceOf<T, I = ()> =
    <<T as VotingConfig<I>>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

// const PALLET_NAME: &'static [u8] = b"CustomVoting";
const ETHEREUM_VOTE: &'static [u8] = b"EthereumVote";
const SIGNED_ETHEREUM_VOTE: &'static [u8] = b"SignedEthereumVote";

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use crate::{default_weights::WeightInfo, BalanceOf, PollIndexOf, VotingConfig, ETHEREUM_VOTE};
    use codec::{Decode, Encode};
    use core::fmt::Debug;
    use frame_support::{
        crypto::ecdsa,
        dispatch::DispatchResult,
        pallet_prelude::*,
        traits::{Polling, Time},
    };
    use frame_system::pallet_prelude::{OriginFor, *};
    use pallet_conviction_voting::AccountVote;
    use scale_info::TypeInfo;
    use sp_avn_common::{
        hash_with_ethereum_prefix, recover_public_key_from_ecdsa_signature, verify_signature, CallDecoder, InnerCallValidator, Proof
    };
    use sp_core::ecdsa::Signature as EcdsaSignature;
    use sp_io::hashing::keccak_256;
    use sp_runtime::{
        traits::{AtLeast32Bit, Zero},
        ArithmeticError, MultiSignature,
    };
    use crate::Signature;


    #[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, Debug)]
    #[scale_info(skip_type_params(T))]
    pub struct VoteProof<T: Config> {
        pub voter: T::AccountId,
        pub vote: AccountVote<BalanceOf<T>>,
        pub timestamp: T::Moment,
        pub ethereum_signature: EcdsaSignature,
        pub ethereum_public_key: T::EthereumPublicKey,
    }

    #[pallet::config]
    pub trait Config: frame_system::Config + VotingConfig + core::fmt::Debug {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        type RuntimeCall: Parameter
            + Dispatchable<RuntimeOrigin = <Self as frame_system::Config>::RuntimeOrigin>
            + IsSubType<Call<Self>>
            + From<Call<Self>>
            + GetDispatchInfo
            + From<frame_system::Call<Self>>;
        type WeightInfo: WeightInfo;
        type TimeProvider: Time<Moment = Self::Moment>;
        type MaxVoteAge: Get<Self::Moment>;
        type Moment: Clone
            + Copy
            + PartialOrd
            + AtLeast32Bit
            + Default
            + From<u64>
            + TypeInfo
            + Debug
            + Encode
            + Decode;
        type EthereumPublicKey: AsRef<[u8]> + Parameter;
        // A type that can be used to verify signatures
        type Public: IdentifyAccount<AccountId = Self::AccountId>;
        /// The signature type used by accounts/transactions.
        type Signature: Verify<Signer = Self::Public>
            + Member
            + Decode
            + Encode
            + From<MultiSignature>
            + TypeInfo;
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    #[pallet::getter(fn processed_votes)]
    pub type ProcessedVotes<T: Config> = StorageDoubleMap<
        _,
        Twox64Concat,
        T::AccountId,
        Twox64Concat,
        PollIndexOf<T>,
        bool,
        ValueQuery,
    >;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        VoteRecorded(T::AccountId, PollIndexOf<T>),
        EthereumVoteProcessed(T::AccountId, PollIndexOf<T>),
        CallDispatched { relayer: T::AccountId, call_hash: T::Hash },
    }

    #[pallet::error]
    pub enum Error<T> {
        MaxVotesReached,
        AlreadyDelegating,
        NotOngoing,
        AlreadyVoted,
        FutureTimestamp,
        VoteTooOld,
        InvalidEthereumSignature,
        UnauthorizedProxyEthereumVote,
        UnauthorizedProxyTransaction,
        TransactionNotSupported,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T>
    where
        VoteProof<T>: Encode + Decode + Debug,
    {
        #[pallet::call_index(0)]
        #[pallet::weight(<T as Config>::WeightInfo::vote())]
        pub fn vote(
            origin: OriginFor<T>,
            poll_index: PollIndexOf<T>,
            vote: AccountVote<BalanceOf<T>>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::do_vote(who, poll_index, vote)
        }

        #[pallet::call_index(1)]
        #[pallet::weight(<T as Config>::WeightInfo::submit_ethereum_vote())]
        pub fn submit_ethereum_vote(
            origin: OriginFor<T>,
            poll_index: PollIndexOf<T>,
            vote_proof: VoteProof<T>,
        ) -> DispatchResult {
            log::info!(
                "submit_ethereum_vote called with poll_index: {:?}, vote_proof: {:?}",
                poll_index,
                vote_proof
            );
            log::info!("Entering submit_ethereum_vote");
            let _ = ensure_signed(origin)?;
            log::info!("Origin signed");

            log::info!("Checking if vote already processed");
            ensure!(
                !ProcessedVotes::<T>::contains_key(&vote_proof.voter, &poll_index),
                Error::<T>::AlreadyVoted
            );
            log::info!("Vote not already processed");

            let now = T::TimeProvider::now();
            log::info!("Current time: {:?}", now);
            log::info!("Vote proof timestamp: {:?}", vote_proof.timestamp);
            log::info!("MaxVoteAge: {:?}", T::MaxVoteAge::get());

            ensure!(vote_proof.timestamp <= now, Error::<T>::FutureTimestamp);
            log::info!("Timestamp is not in the future");

            // Check if the subtraction will underflow
            if now < vote_proof.timestamp {
                log::error!("Time difference calculation would underflow");
                return Err(Error::<T>::VoteTooOld.into())
            }

            let time_diff = now - vote_proof.timestamp;
            log::info!("Time difference: {:?}", time_diff);

            ensure!(time_diff <= T::MaxVoteAge::get(), Error::<T>::VoteTooOld);
            log::info!("Vote is not too old");
            log::info!("Constructing message to sign");
            let message =
                Self::construct_vote_message(poll_index, &vote_proof.vote, vote_proof.timestamp);

            log::info!("About to validate Ethereum signature");
            ensure!(
                Self::eth_signature_is_valid(
                    message,
                    &vote_proof.ethereum_public_key,
                    &vote_proof.ethereum_signature,
                ),
                Error::<T>::InvalidEthereumSignature
            );
            log::info!("Ethereum signature is valid");

            Self::do_vote(vote_proof.voter.clone(), poll_index, vote_proof.vote)?;

            ProcessedVotes::<T>::insert(&vote_proof.voter, &poll_index, true);

            Self::deposit_event(Event::EthereumVoteProcessed(vote_proof.voter, poll_index));
            Ok(())
        }

        #[pallet::call_index(2)]
        #[pallet::weight(<T as Config>::WeightInfo::submit_ethereum_vote())]
        pub fn proxy(
            origin: OriginFor<T>,
            call: Box<<T as Config>::RuntimeCall>,
        ) -> DispatchResult {
            let relayer = ensure_signed(origin)?;

            let proof = Self::get_proof(&*call)?;
            ensure!(relayer == proof.relayer, Error::<T>::UnauthorizedProxyTransaction);

            let call_hash: T::Hash = T::Hashing::hash_of(&call);
            call.dispatch(frame_system::RawOrigin::Signed(proof.signer).into())
                .map(|_| ())
                .map_err(|e| e.error)?;
            Self::deposit_event(Event::<T>::CallDispatched { relayer, call_hash });
            Ok(())
        }

        #[pallet::call_index(3)]
        #[pallet::weight(<T as Config>::WeightInfo::submit_ethereum_vote())]
        pub fn signed_ethereum_vote(
            origin: OriginFor<T>,
            proof: Proof<T::Signature, T::AccountId>,
            poll_index: PollIndexOf<T>,
            vote_proof: VoteProof<T>,
        ) -> DispatchResult {
            let sender = ensure_signed(origin)?;

            let signed_payload =
                Self::encode_signed_vote_params(&proof, poll_index, &vote_proof.clone());

            ensure!(
                verify_signature::<T::Signature, T::AccountId>(&proof, &signed_payload.as_slice())
                    .is_ok(),
                Error::<T>::UnauthorizedProxyEthereumVote
            );

            Self::do_vote(vote_proof.voter.clone(), poll_index, vote_proof.vote)?;

            ProcessedVotes::<T>::insert(&vote_proof.voter, &poll_index, true);

            Self::deposit_event(Event::EthereumVoteProcessed(vote_proof.voter, poll_index));
            Ok(())
        }
    }

    impl<T: Config> Pallet<T>
    where
        VoteProof<T>: Encode + Decode + Debug,
    {
        fn do_vote(
            who: T::AccountId,
            poll_index: PollIndexOf<T>,
            vote: AccountVote<BalanceOf<T>>,
        ) -> DispatchResult {
            <T as VotingConfig>::Polls::try_access_poll(poll_index, |poll_status| {
                let (tally, class) = poll_status.ensure_ongoing().ok_or(Error::<T>::NotOngoing)?;
                pallet_conviction_voting::VotingFor::<T, ()>::try_mutate(
                    who.clone(),
                    &class,
                    |voting| {
                        if let pallet_conviction_voting::Voting::Casting(
                            pallet_conviction_voting::Casting {
                                ref mut votes, delegations, ..
                            },
                        ) = voting
                        {
                            match votes.binary_search_by_key(&poll_index, |i| i.0) {
                                Ok(i) => {
                                    tally.remove(votes[i].1).ok_or(ArithmeticError::Underflow)?;
                                    if let Some(approve) = votes[i].1.as_standard() {
                                        tally.reduce(approve, *delegations);
                                    }
                                    votes[i].1 = vote;
                                },
                                Err(i) => {
                                    votes
                                        .try_insert(i, (poll_index, vote))
                                        .map_err(|_| Error::<T>::MaxVotesReached)?;
                                },
                            }
                            tally.add(vote).ok_or(ArithmeticError::Overflow)?;
                            if let Some(approve) = vote.as_standard() {
                                tally.increase(approve, *delegations);
                            }
                        } else {
                            return Err(Error::<T>::AlreadyDelegating.into())
                        }
                        Ok(())
                    },
                )
            })?;

            Self::deposit_event(Event::VoteRecorded(who, poll_index));
            Ok(())
        }

        fn construct_vote_message(
            poll_index: PollIndexOf<T>,
            vote: &AccountVote<BalanceOf<T>>,
            timestamp: <<T as Config>::TimeProvider as Time>::Moment,
        ) -> Vec<u8> {
            log::info!("Rust - Constructing vote message");
            log::info!("Rust - Poll Index: {:?}", poll_index);
            log::info!("Rust - Vote: {:?}", vote);
            log::info!("Rust - Timestamp: {:?}", timestamp);

            let vote_type_hash = keccak_256(b"Vote(uint256 pollIndex,int8 voteType,uint256 aye,uint256 nay,uint256 abstain,uint256 timestamp)");
            log::info!("Rust - Vote Type Hash: {:?}", hex::encode(vote_type_hash));

            let (vote_type, aye, nay, abstain) = match vote {
                AccountVote::Standard { vote, balance } => {
                    log::info!("Rust - Vote type: Standard");
                    (
                        1i8,
                        if vote.aye { *balance } else { Zero::zero() },
                        if !vote.aye { *balance } else { Zero::zero() },
                        Zero::zero(),
                    )
                },
                AccountVote::Split { aye, nay } => {
                    log::info!("Rust - Vote type: Split");
                    (2i8, *aye, *nay, Zero::zero())
                },
                AccountVote::SplitAbstain { aye, nay, abstain } => {
                    log::info!("Rust - Vote type: SplitAbstain");
                    (3i8, *aye, *nay, *abstain)
                },
            };

            log::info!("Rust - Vote Type: {:?}", vote_type);
            log::info!("Rust - Aye: {:?}", aye);
            log::info!("Rust - Nay: {:?}", nay);
            log::info!("Rust - Abstain: {:?}", abstain);

            let vote_data = [
                poll_index.encode(),
                vote_type.encode(),
                aye.encode(),
                nay.encode(),
                abstain.encode(),
                timestamp.encode(),
            ]
            .concat();

            log::info!("Rust - Vote Data: {:?}", hex::encode(&vote_data));

            let vote_hash =
                keccak_256(&[vote_type_hash.to_vec(), keccak_256(&vote_data).to_vec()].concat());
            log::info!("Rust - Vote Hash: {:?}", hex::encode(vote_hash));

            // let message = [ETHEREUM_VOTE, &vote_hash].concat();
            // log::info!("Rust - Message: {:?}", hex::encode(&message));

            // let final_hash = keccak_256(&vote_hash);
            let final_hash = vote_hash;
            log::info!("Rust - Final Hash: {:?}", hex::encode(final_hash));
            log::info!("Ethereum Prefixed Hash: {}", hex::encode(hash_with_ethereum_prefix(hex::encode(final_hash)).unwrap()));

            final_hash.to_vec()
        }

        fn eth_signature_is_valid(data: Vec<u8>, public_key: &T::EthereumPublicKey, signature: &EcdsaSignature) -> bool {
            let message_hash = keccak_256(&data);
            log::info!("Data to hash: {:?}", hex::encode(&data));
            log::info!("Message hash: {:?}", hex::encode(&message_hash));
        
            // let eth_prefixed_hash = hash_with_ethereum_prefix(&message_hash);
            // log::info!("Ethereum Prefixed Hash: {:?}", hex::encode(&eth_prefixed_hash));
        
            log::info!("Provided signature: {:?}", hex::encode(signature));
            log::info!("Provided public key: {:?}", hex::encode(public_key.as_ref()));
        
            // let ecdsa_sig = ecdsa::Signature::from_slice(signature.as_ref())
            //     .expect("Invalid signature format");
        
            // let recovered_pub_key = recover_public_key_from_ecdsa_signature(ecdsa_sig, hex::encode(eth_prefixed_hash));
            let recovered_pub_key = recover_public_key_from_ecdsa_signature(signature.clone(), hex::encode(data));
            
            match recovered_pub_key {
                Ok(recovered_key) => {
                    log::info!("Recovered public key: {:?}", hex::encode(recovered_key.as_ref()));
                    let is_valid = recovered_key.as_ref() == public_key.as_ref();
                    log::info!("Signature is valid: {}", is_valid);
                    is_valid
                },
                Err(e) => {
                    log::error!("Failed to recover public key from signature: {:?}", e);
                    false
                }
            }
        }

        fn encode_signed_vote_params(
            proof: &Proof<T::Signature, T::AccountId>,
            poll_index: PollIndexOf<T>,
            vote_proof: &VoteProof<T>,
        ) -> Vec<u8> {
            (SIGNED_ETHEREUM_VOTE, proof.relayer.clone(), poll_index, vote_proof.clone()).encode()
        }

        fn get_encoded_call_param(
            call: &<T as Config>::RuntimeCall,
        ) -> Option<(&Proof<T::Signature, T::AccountId>, PollIndexOf<T>, VoteProof<T>, Vec<u8>)>
        {
            let call = match call.is_sub_type() {
                Some(call) => call,
                None => return None,
            };

            match call {
                Call::signed_ethereum_vote { proof, poll_index, vote_proof } => {
                    let encoded_data =
                        Self::encode_signed_vote_params(proof, *poll_index, &vote_proof);
                    Some((proof, *poll_index, vote_proof.clone(), encoded_data))
                },
                _ => None,
            }
        }
    }

    impl<T: Config> CallDecoder for Pallet<T> {
        type AccountId = T::AccountId;
        type Signature = <T as Config>::Signature;
        type Error = Error<T>;
        type Call = <T as Config>::RuntimeCall;

        fn get_proof(
            call: &Self::Call,
        ) -> Result<Proof<Self::Signature, Self::AccountId>, Self::Error> {
            let call = match call.is_sub_type() {
                Some(call) => call,
                None => return Err(Error::TransactionNotSupported),
            };

            match call {
                Call::signed_ethereum_vote { proof, .. } => return Ok(proof.clone()),
                _ => return Err(Error::TransactionNotSupported),
            }
        }
    }

    impl<T: Config> InnerCallValidator for Pallet<T> {
        type Call = <T as Config>::RuntimeCall;

        fn signature_is_valid(call: &Box<Self::Call>) -> bool {
            if let Some((proof, _, _, signed_payload)) = Self::get_encoded_call_param(call) {
                return verify_signature::<T::Signature, T::AccountId>(
                    &proof,
                    &signed_payload.as_slice(),
                )
                .is_ok()
            }
            false
        }
    }
}