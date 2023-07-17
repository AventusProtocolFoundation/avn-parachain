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

//! # Token manager pallet

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
use core::convert::{TryFrom, TryInto};
use frame_support::{
    dispatch::{DispatchResult, DispatchResultWithPostInfo, GetDispatchInfo},
    ensure, log,
    traits::{Currency, ExistenceRequirement, Get, Imbalance, IsSubType, WithdrawReasons},
    PalletId, Parameter,
};
use frame_system::ensure_signed;
pub use pallet::*;
use pallet_avn::{
    self as avn, CollatorPayoutDustHandler, OnGrowthLiftedHandler, ProcessedEventsChecker,
};
use sp_avn_common::{
    event_types::{AvtGrowthLiftedData, EthEvent, EventData, LiftedData, ProcessedEventHandler},
    verify_signature, CallDecoder, InnerCallValidator, Proof,
};
use sp_core::{H160, H256};
use sp_runtime::{
    scale_info::TypeInfo,
    traits::{
        AccountIdConversion, AtLeast32Bit, CheckedAdd, Dispatchable, Hash, IdentifyAccount, Member,
        Saturating, Verify, Zero,
    },
    Perbill,
};
use sp_std::prelude::*;

type BalanceOf<T> =
    <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;
type PositiveImbalanceOf<T> = <<T as Config>::Currency as Currency<
    <T as frame_system::Config>::AccountId,
>>::PositiveImbalance;

mod benchmarking;

pub mod default_weights;
pub use default_weights::WeightInfo;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod test_proxying_signed_transfer;

#[cfg(test)]
mod test_proxying_signed_lower;

#[cfg(test)]
mod test_common_cases;

#[cfg(test)]
mod test_avt_tokens;

#[cfg(test)]
mod test_non_avt_tokens;

#[cfg(test)]
mod test_growth;

pub const SIGNED_TRANSFER_CONTEXT: &'static [u8] = b"authorization for transfer operation";
pub const SIGNED_LOWER_CONTEXT: &'static [u8] = b"authorization for lower operation";

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::{pallet_prelude::*, Blake2_128Concat};
    use frame_system::{ensure_root, pallet_prelude::*};

    // Public interface of this pallet
    #[pallet::config]
    pub trait Config: frame_system::Config + avn::Config {
        /// The overarching event type.
        type RuntimeEvent: From<Event<Self>>
            + Into<<Self as frame_system::Config>::RuntimeEvent>
            + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// The overarching call type.
        type RuntimeCall: Parameter
            + Dispatchable<RuntimeOrigin = <Self as frame_system::Config>::RuntimeOrigin>
            + IsSubType<Call<Self>>
            + From<Call<Self>>
            + GetDispatchInfo;

        /// Currency type for lifting
        type Currency: Currency<Self::AccountId>;

        /// The units in which we record balances of tokens others than AVT
        type TokenBalance: Member + Parameter + AtLeast32Bit + Default + Copy + MaxEncodedLen;

        /// The type of token identifier
        /// (a H160 because this is an Ethereum address)
        type TokenId: Parameter + Default + Copy + From<H160> + MaxEncodedLen;

        type ProcessedEventsChecker: ProcessedEventsChecker;

        /// A type that can be used to verify signatures
        type Public: IdentifyAccount<AccountId = Self::AccountId>;

        /// The signature type used by accounts/transactions.
        type Signature: Verify<Signer = Self::Public>
            + Member
            + Decode
            + Encode
            + From<sp_core::sr25519::Signature>
            + TypeInfo;

        /// Id of the account that will hold treasury funds
        type AvnTreasuryPotId: Get<PalletId>;

        /// Percentage of growth to store in the treasury
        #[pallet::constant]
        type TreasuryGrowthPercentage: Get<Perbill>;

        /// Handler to notify the runtime when AVT growth is lifted.
        type OnGrowthLiftedHandler: OnGrowthLiftedHandler<BalanceOf<Self>>;

        type WeightInfo: WeightInfo;
    }

    #[pallet::pallet]
    #[pallet::generate_store(pub (super) trait Store)]
    #[pallet::without_storage_info]
    pub struct Pallet<T>(_);

    #[pallet::event]
    /// This attribute generate the function `deposit_event` to deposit one of this pallet event,
    /// it is optional, it is also possible to provide a custom implementation.
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        AVTLifted {
            recipient: T::AccountId,
            amount: BalanceOf<T>,
            eth_tx_hash: H256,
        },
        TokenLifted {
            token_id: T::TokenId,
            recipient: T::AccountId,
            token_balance: T::TokenBalance,
            eth_tx_hash: H256,
        },
        TokenTransferred {
            token_id: T::TokenId,
            sender: T::AccountId,
            recipient: T::AccountId,
            token_balance: T::TokenBalance,
        },
        CallDispatched {
            relayer: T::AccountId,
            call_hash: T::Hash,
        },
        TokenLowered {
            token_id: T::TokenId,
            sender: T::AccountId,
            recipient: T::AccountId,
            amount: u128,
            t1_recipient: H160,
        },
        AvtTransferredFromTreasury {
            recipient: T::AccountId,
            amount: BalanceOf<T>,
        },
        AVTGrowthLifted {
            treasury_share: BalanceOf<T>,
            collators_share: BalanceOf<T>,
            eth_tx_hash: H256,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        NoTier1EventForLogLifted,
        AmountOverflow,
        DepositFailed,
        LowerFailed,
        AmountIsZero,
        InsufficientSenderBalance,
        TransactionNotSupported,
        SenderNotValid,
        UnauthorizedTransaction,
        UnauthorizedProxyTransaction,
        UnauthorizedSignedTransferTransaction,
        UnauthorizedSignedLowerTransaction,
        ErrorConvertingAccountId,
        ErrorConvertingTokenBalance,
        ErrorConvertingToBalance,
        NoTier1EventForLogAvtGrowthLifted,
    }

    #[pallet::storage]
    #[pallet::getter(fn balance)]
    /// The number of units of tokens held by any given account.
    pub type Balances<T: Config> =
        StorageMap<_, Blake2_128Concat, (T::TokenId, T::AccountId), T::TokenBalance, ValueQuery>;

    /// An account nonce that represents the number of transfers from this account
    /// It is shared for all tokens held by the account
    #[pallet::storage]
    #[pallet::getter(fn nonce)]
    pub type Nonces<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, u64, ValueQuery>;

    /// An account without a known private key, that can send transfers (eg Lowering transfers) but
    /// from which no one can send funds. Tokens sent to this account are effectively destroyed.
    #[pallet::storage]
    #[pallet::getter(fn lower_account_id)]
    pub type LowerAccountId<T: Config> = StorageValue<_, H256, ValueQuery>;

    /// The ethereum address of the AVT contract. Default value is the Rinkeby address
    #[pallet::storage]
    #[pallet::getter(fn avt_token_contract)]
    pub type AVTTokenContract<T: Config> = StorageValue<_, H160, ValueQuery>;

    #[pallet::genesis_config]
    pub struct GenesisConfig<T: Config> {
        pub _phantom: sp_std::marker::PhantomData<T>,
        pub lower_account_id: H256,
        pub avt_token_contract: H160,
    }

    #[cfg(feature = "std")]
    impl<T: Config> Default for GenesisConfig<T> {
        fn default() -> Self {
            Self {
                _phantom: Default::default(),
                lower_account_id: H256::zero(),
                avt_token_contract: H160::zero(),
            }
        }
    }

    #[pallet::genesis_build]
    impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
        fn build(&self) {
            <LowerAccountId<T>>::put(self.lower_account_id);
            <AVTTokenContract<T>>::put(self.avt_token_contract);
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// This extrinsic allows relayer to dispatch a `signed_transfer` or `signed_lower` call for
        /// a sender. As a general rule, every function that can be proxied should follow
        /// this convention:
        /// - its first argument (after origin) should be a public verification key and a signature
        #[pallet::call_index(0)]
        #[pallet::weight(<T as pallet::Config>::WeightInfo::proxy_with_non_avt_token().saturating_add(call.get_dispatch_info().weight))]
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

        /// Transfer an amount of token with token_id from sender to receiver with a proof
        #[pallet::call_index(1)]
        #[pallet::weight(<T as pallet::Config>::WeightInfo::signed_transfer())]
        pub fn signed_transfer(
            origin: OriginFor<T>,
            proof: Proof<T::Signature, T::AccountId>,
            from: T::AccountId,
            to: T::AccountId,
            token_id: T::TokenId,
            amount: T::TokenBalance,
        ) -> DispatchResult {
            let sender = ensure_signed(origin)?;
            ensure!(sender == from, Error::<T>::SenderNotValid);
            let sender_nonce = Self::nonce(&sender);

            let signed_payload = Self::encode_signed_transfer_params(
                &proof,
                &from,
                &to,
                &token_id,
                &amount,
                sender_nonce,
            );

            ensure!(
                verify_signature::<T::Signature, T::AccountId>(&proof, &signed_payload.as_slice())
                    .is_ok(),
                Error::<T>::UnauthorizedSignedTransferTransaction
            );

            Self::settle_transfer(&token_id, &from, &to, &amount)?;

            Ok(())
        }

        /// Lower an amount of token from tier2 to tier1
        #[pallet::weight(<T as pallet::Config>::WeightInfo::lower_avt_token().max(<T as pallet::Config>::WeightInfo::lower_non_avt_token()))]
        #[pallet::call_index(2)]
        pub fn lower(
            origin: OriginFor<T>,
            from: T::AccountId,
            token_id: T::TokenId,
            amount: u128,
            t1_recipient: H160, // the receiver address on tier1
        ) -> DispatchResultWithPostInfo {
            let sender = ensure_signed(origin)?;
            ensure!(sender == from, Error::<T>::SenderNotValid);
            ensure!(amount != 0, Error::<T>::AmountIsZero);

            let to_account_id = T::AccountId::decode(&mut Self::lower_account_id().as_bytes())
                .map_err(|_| Error::<T>::ErrorConvertingAccountId)?;

            Self::settle_lower(token_id, &from, &to_account_id, amount, t1_recipient)?;

            let final_weight = if token_id == Self::avt_token_contract().into() {
                <T as pallet::Config>::WeightInfo::lower_avt_token()
            } else {
                <T as pallet::Config>::WeightInfo::lower_non_avt_token()
            };

            Ok(Some(final_weight).into())
        }

        /// Lower an amount of token from tier2 to tier1 by a relayer
        #[pallet::weight(<T as pallet::Config>::WeightInfo::signed_lower_avt_token().max(<T as pallet::Config>::WeightInfo::signed_lower_non_avt_token()))]
        #[pallet::call_index(3)]
        pub fn signed_lower(
            origin: OriginFor<T>,
            proof: Proof<T::Signature, T::AccountId>,
            from: T::AccountId,
            token_id: T::TokenId,
            amount: u128,
            t1_recipient: H160, // the receiver address on tier1
        ) -> DispatchResultWithPostInfo {
            let sender = ensure_signed(origin)?;
            ensure!(sender == from, Error::<T>::SenderNotValid);
            ensure!(amount != 0, Error::<T>::AmountIsZero);

            let sender_nonce = Self::nonce(&sender);
            let signed_payload = Self::encode_signed_lower_params(
                &proof,
                &from,
                &token_id,
                &amount,
                &t1_recipient,
                sender_nonce,
            );

            ensure!(
                verify_signature::<T::Signature, T::AccountId>(&proof, &signed_payload.as_slice())
                    .is_ok(),
                Error::<T>::UnauthorizedSignedLowerTransaction
            );

            let to_account_id = T::AccountId::decode(&mut Self::lower_account_id().as_bytes())
                .map_err(|_| Error::<T>::ErrorConvertingAccountId)?;

            Self::settle_lower(token_id, &from, &to_account_id, amount, t1_recipient)?;

            let final_weight = if token_id == Self::avt_token_contract().into() {
                <T as pallet::Config>::WeightInfo::signed_lower_avt_token()
            } else {
                <T as pallet::Config>::WeightInfo::signed_lower_non_avt_token()
            };

            Ok(Some(final_weight).into())
        }

        /// Transfer AVT from the treasury account. The origin must be root.
        // TODO: benchmark me
        #[pallet::call_index(4)]
        #[pallet::weight(<T as pallet::Config>::WeightInfo::transfer_from_treasury())]
        pub fn transfer_from_treasury(
            origin: OriginFor<T>,
            recipient: T::AccountId,
            amount: BalanceOf<T>,
        ) -> DispatchResult {
            ensure_root(origin)?;
            ensure!(amount != BalanceOf::<T>::zero(), Error::<T>::AmountIsZero);

            <T as pallet::Config>::Currency::transfer(
                &Self::compute_treasury_account_id(),
                &recipient,
                amount,
                ExistenceRequirement::KeepAlive,
            )?;

            Self::deposit_event(Event::<T>::AvtTransferredFromTreasury { recipient, amount });

            Ok(())
        }
    }
}

impl<T: Config> Pallet<T> {
    fn settle_transfer(
        token_id: &T::TokenId,
        from: &T::AccountId,
        to: &T::AccountId,
        amount: &T::TokenBalance,
    ) -> DispatchResult {
        if *token_id == Self::avt_token_contract().into() {
            // First convert TokenBalance to u128
            let amount_u128 = TryInto::<u128>::try_into(*amount)
                .map_err(|_| Error::<T>::ErrorConvertingTokenBalance)?;
            // Then convert to Balance
            let transfer_amount = <BalanceOf<T> as TryFrom<u128>>::try_from(amount_u128)
                .or_else(|_error| Err(Error::<T>::ErrorConvertingToBalance))?;

            <T as pallet::Config>::Currency::transfer(
                from,
                to,
                transfer_amount,
                ExistenceRequirement::KeepAlive,
            )?;
        } else {
            let sender_balance = Self::balance((token_id, from));
            ensure!(sender_balance >= *amount, Error::<T>::InsufficientSenderBalance);

            if from != to {
                // If we are transfering to ourselves, we need to be careful when reading the
                // balance because `Self::balance((token_id, from))` ==
                // `Self::balance((token_id, to))` hence the if statement.
                let receiver_balance = Self::balance((token_id, to));
                ensure!(receiver_balance.checked_add(amount).is_some(), Error::<T>::AmountOverflow);
            }

            <Balances<T>>::mutate((token_id, from), |balance| *balance -= *amount);

            <Balances<T>>::mutate((token_id, to), |balance| *balance += *amount);

            Self::deposit_event(Event::<T>::TokenTransferred {
                token_id: token_id.clone(),
                sender: from.clone(),
                recipient: to.clone(),
                token_balance: amount.clone(),
            });
        }

        <Nonces<T>>::mutate(from, |n| *n += 1);

        Ok(())
    }

    fn lift(event: &EthEvent) -> DispatchResult {
        return match &event.event_data {
            EventData::LogLifted(d) => return Self::process_lift(event, d),
            EventData::LogAvtGrowthLifted(d) => return Self::process_avt_growth_lift(event, d),

            // Event handled or it is not for us, in which case ignore it.
            _ => Ok(()),
        }
    }

    fn settle_lower(
        token_id: T::TokenId,
        from: &T::AccountId,
        to: &T::AccountId,
        amount: u128,
        t1_recipient: H160,
    ) -> DispatchResult {
        if token_id == Self::avt_token_contract().into() {
            let lower_amount = <BalanceOf<T> as TryFrom<u128>>::try_from(amount)
                .or_else(|_error| Err(Error::<T>::AmountOverflow))?;
            // Note: Keep account alive when balance is lower than existence requirement,
            // so the SystemNonce will not be reset just in case if any logic relies on the
            // SystemNonce. However all zero AVT account balances will be kept in our
            // runtime storage.
            let imbalance = <T as pallet::Config>::Currency::withdraw(
                &from,
                lower_amount,
                WithdrawReasons::TRANSFER,
                ExistenceRequirement::KeepAlive,
            )?;

            if imbalance.peek() == BalanceOf::<T>::zero() {
                Err(Error::<T>::LowerFailed)?
            }

            // Decreases the total issued AVT when this negative imbalance is dropped
            // so that total issued AVT becomes equal to total supply once again.
            drop(imbalance);
        } else {
            let lower_amount = <T::TokenBalance as TryFrom<u128>>::try_from(amount)
                .or_else(|_error| Err(Error::<T>::AmountOverflow))?;
            let sender_balance = Self::balance((token_id, from));
            ensure!(sender_balance >= lower_amount, Error::<T>::InsufficientSenderBalance);

            <Balances<T>>::mutate((token_id, from), |balance| *balance -= lower_amount);

            Self::deposit_event(Event::<T>::TokenLowered {
                token_id,
                sender: from.clone(),
                recipient: to.clone(),
                amount,
                t1_recipient,
            });
        }

        <Nonces<T>>::mutate(from, |n| *n += 1);

        Ok(())
    }

    fn update_token_balance(
        transaction_hash: H256,
        token_id: T::TokenId,
        recipient_account_id: T::AccountId,
        raw_amount: u128,
    ) -> DispatchResult {
        let amount = <T::TokenBalance as TryFrom<u128>>::try_from(raw_amount)
            .or_else(|_error| Err(Error::<T>::AmountOverflow))?;

        if <Balances<T>>::contains_key((token_id, &recipient_account_id)) {
            Self::increment_token_balance(token_id, &recipient_account_id, &amount)?;
        } else {
            <Balances<T>>::insert((token_id, &recipient_account_id), amount);
        }

        Self::deposit_event(Event::<T>::TokenLifted {
            token_id,
            recipient: recipient_account_id,
            token_balance: amount,
            eth_tx_hash: transaction_hash,
        });

        Ok(())
    }

    fn update_avt_balance(
        recipient_account_id: &T::AccountId,
        raw_amount: u128,
    ) -> Result<BalanceOf<T>, Error<T>> {
        let amount = <BalanceOf<T> as TryFrom<u128>>::try_from(raw_amount)
            .or_else(|_error| Err(Error::<T>::AmountOverflow))?;

        // Drop the imbalance caused by depositing amount into the recipient account without a
        // corresponding deduction.  If the recipient account does not exist,
        // deposit_creating function will create a new one.
        let imbalance: PositiveImbalanceOf<T> =
            <T as pallet::Config>::Currency::deposit_creating(recipient_account_id, amount);

        if imbalance.peek() == BalanceOf::<T>::zero() {
            Err(Error::<T>::DepositFailed)?
        }

        // Increases the total issued AVT when this positive imbalance is dropped
        // so that total issued AVT becomes equal to total supply once again.
        drop(imbalance);

        Ok(amount)
    }

    fn increment_token_balance(
        token_id: T::TokenId,
        recipient_account_id: &T::AccountId,
        amount: &T::TokenBalance,
    ) -> DispatchResult {
        let current_balance = Self::balance((token_id, recipient_account_id));
        let new_balance = current_balance.checked_add(amount).ok_or(Error::<T>::AmountOverflow)?;

        <Balances<T>>::mutate((token_id, recipient_account_id), |balance| *balance = new_balance);

        Ok(())
    }

    fn encode_signed_transfer_params(
        proof: &Proof<T::Signature, T::AccountId>,
        from: &T::AccountId,
        to: &T::AccountId,
        token_id: &T::TokenId,
        amount: &T::TokenBalance,
        sender_nonce: u64,
    ) -> Vec<u8> {
        return (
            SIGNED_TRANSFER_CONTEXT,
            proof.relayer.clone(),
            from,
            to,
            token_id,
            amount,
            sender_nonce,
        )
            .encode()
    }

    fn encode_signed_lower_params(
        proof: &Proof<T::Signature, T::AccountId>,
        from: &T::AccountId,
        token_id: &T::TokenId,
        amount: &u128,
        t1_recipient: &H160,
        sender_nonce: u64,
    ) -> Vec<u8> {
        return (
            SIGNED_LOWER_CONTEXT,
            proof.relayer.clone(),
            from,
            token_id,
            amount,
            t1_recipient,
            sender_nonce,
        )
            .encode()
    }

    fn get_encoded_call_param(
        call: &<T as Config>::RuntimeCall,
    ) -> Option<(&Proof<T::Signature, T::AccountId>, Vec<u8>)> {
        let call = match call.is_sub_type() {
            Some(call) => call,
            None => return None,
        };

        match call {
            Call::signed_transfer { proof, from, to, token_id, amount } => {
                let sender_nonce = Self::nonce(&proof.signer);
                let encoded_data = Self::encode_signed_transfer_params(
                    proof,
                    from,
                    to,
                    token_id,
                    amount,
                    sender_nonce,
                );

                return Some((proof, encoded_data))
            },
            Call::signed_lower { proof, from, token_id, amount, t1_recipient } => {
                let sender_nonce = Self::nonce(&proof.signer);
                let encoded_data = Self::encode_signed_lower_params(
                    proof,
                    from,
                    token_id,
                    amount,
                    t1_recipient,
                    sender_nonce,
                );

                return Some((proof, encoded_data))
            },
            _ => return None,
        }
    }

    fn process_lift(event: &EthEvent, data: &LiftedData) -> DispatchResult {
        let event_id = &event.event_id;
        let recipient_account_id = T::AccountId::decode(&mut data.receiver_address.as_bytes())
            .expect("32 bytes will always decode into an AccountId");

        let event_validity = T::ProcessedEventsChecker::check_event(event_id);
        ensure!(event_validity, Error::<T>::NoTier1EventForLogLifted);

        if data.amount == 0 {
            Err(Error::<T>::AmountIsZero)?
        }

        if data.token_contract == Self::avt_token_contract() {
            let updated_amount = Self::update_avt_balance(&recipient_account_id, data.amount)?;

            Self::deposit_event(Event::<T>::AVTLifted {
                recipient: recipient_account_id.clone(),
                amount: updated_amount,
                eth_tx_hash: event_id.transaction_hash,
            });
        } else {
            Self::update_token_balance(
                event_id.transaction_hash,
                data.token_contract.into(),
                recipient_account_id,
                data.amount,
            )?;
        }

        return Ok(())
    }

    fn process_avt_growth_lift(event: &EthEvent, data: &AvtGrowthLiftedData) -> DispatchResult {
        let event_id = &event.event_id;
        let event_validity = T::ProcessedEventsChecker::check_event(event_id);
        ensure!(event_validity, Error::<T>::NoTier1EventForLogAvtGrowthLifted);

        if data.amount == 0 {
            Err(Error::<T>::AmountIsZero)?
        }

        let treasury_share = T::TreasuryGrowthPercentage::get() * data.amount;

        // Send a portion of the funds to the treasury
        let treasury_amount =
            Self::update_avt_balance(&Self::compute_treasury_account_id(), treasury_share)?;

        // Now let the runtime know we have a lift so we can payout collators
        let remaining_amount =
            <BalanceOf<T> as TryFrom<u128>>::try_from(data.amount - treasury_share)
                .or_else(|_error| Err(Error::<T>::AmountOverflow))?;

        Self::deposit_event(Event::<T>::AVTGrowthLifted {
            treasury_share: treasury_amount,
            collators_share: remaining_amount,
            eth_tx_hash: event_id.transaction_hash,
        });

        T::OnGrowthLiftedHandler::on_growth_lifted(remaining_amount.into(), data.period)?;

        Ok(())
    }

    /// The account ID of the AvN treasury.
    /// This actually does computation. If you need to keep using it, then make sure you cache
    /// the value and only call this once.
    pub fn compute_treasury_account_id() -> T::AccountId {
        T::AvnTreasuryPotId::get().into_account_truncating()
    }

    /// The total amount of funds stored in this pallet
    pub fn treasury_balance() -> BalanceOf<T> {
        // Must never be less than 0 but better be safe.
        <T as pallet::Config>::Currency::free_balance(&Self::compute_treasury_account_id())
            .saturating_sub(<T as pallet::Config>::Currency::minimum_balance())
    }
}

impl<T: Config> ProcessedEventHandler for Pallet<T> {
    fn on_event_processed(event: &EthEvent) -> DispatchResult {
        return Self::lift(event)
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
            Call::signed_transfer { proof, .. } => return Ok(proof.clone()),
            Call::signed_lower { proof, .. } => return Ok(proof.clone()),
            _ => return Err(Error::TransactionNotSupported),
        }
    }
}

impl<T: Config> InnerCallValidator for Pallet<T> {
    type Call = <T as Config>::RuntimeCall;

    fn signature_is_valid(call: &Box<Self::Call>) -> bool {
        if let Some((proof, signed_payload)) = Self::get_encoded_call_param(call) {
            return verify_signature::<T::Signature, T::AccountId>(
                &proof,
                &signed_payload.as_slice(),
            )
            .is_ok()
        }

        return false
    }
}

// Deal with any positive imbalance by sending it to the treasury
impl<T: Config> CollatorPayoutDustHandler<BalanceOf<T>> for Pallet<T> {
    fn handle_dust(imbalance: BalanceOf<T>) {
        if let Err(e) =
            T::Currency::deposit_into_existing(&Self::compute_treasury_account_id(), imbalance)
        {
            log::error!("💔💔 Error transferring {:?} AVT to treasury : {:?}", imbalance, e);
        }

        // If the deposit succeeds, when this function goes out of scope, total issuance will
        // increase by "imbalance"
    }
}
