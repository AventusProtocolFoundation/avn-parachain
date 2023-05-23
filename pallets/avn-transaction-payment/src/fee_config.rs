use crate::{BalanceOf, Config, Error};
use codec::{Decode, Encode, MaxEncodedLen};
use sp_runtime::{scale_info::TypeInfo, traits::Zero, Perbill, Saturating};
use sp_std::{fmt::Debug, marker::PhantomData};

#[derive(Encode, Decode, MaxEncodedLen, Clone, PartialEq, Eq, TypeInfo, Copy)]
#[scale_info(skip_type_params(T))]
pub enum FeeType<T: Config> {
    FixedFee(FixedFeeConfig<T>),
    PercentageFee(PercentageFeeConfig<T>),
    Unknown,
}

impl<T: Config> Debug for FeeType<T> {
    fn fmt(&self, f: &mut sp_std::fmt::Formatter<'_>) -> sp_std::fmt::Result {
        match self {
            Self::FixedFee(c) => {
                write!(f, "Fixed fee[{:?}]", c.fee)
            },
            Self::PercentageFee(c) => {
                write!(f, "Percentage fee[{}]", c.percentage)
            },
            Self::Unknown => {
                write!(f, "Unknwon fee type")
            },
        }
    }
}

impl<T: Config> Default for FeeType<T> {
    fn default() -> Self {
        FeeType::Unknown
    }
}

#[derive(Encode, Decode, MaxEncodedLen, Clone, PartialEq, Eq, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub enum FeeConfig<T: Config> {
    FixedFee(FixedFeeConfig<T>),
    PercentageFee(PercentageFeeConfig<T>),
    TimeBased(TimeBasedConfig<T>),
    TransactionBased(TransactionBasedConfig<T>),
    Unknown,
}

impl<T: Config> Debug for FeeConfig<T> {
    fn fmt(&self, f: &mut sp_std::fmt::Formatter<'_>) -> sp_std::fmt::Result {
        match self {
            Self::FixedFee(c) => {
                write!(f, "Fixed fee[{:?}]", c.fee)
            },
            Self::PercentageFee(c) => {
                write!(f, "Percentage fee[{}]", c.percentage)
            },
            Self::TimeBased(c) => {
                write!(
                    f,
                    "Time based fee[{:?}, {:?}, {:?}]",
                    c.duration, c.end_block_number, c.fee_type
                )
            },
            Self::TransactionBased(c) => {
                write!(
                    f,
                    "Transaction based fee[{:?}, {:?}, {:?}, {:?}]",
                    c.account, c.count, c.end_count, c.fee_type
                )
            },
            Self::Unknown => {
                write!(f, "Fee config unknown")
            },
        }
    }
}

impl<T: Config> Default for FeeConfig<T> {
    fn default() -> Self {
        FeeConfig::Unknown
    }
}

impl<T: Config> FeeConfig<T> {
    pub fn is_valid(&self) -> bool {
        return match self {
            FeeConfig::FixedFee(c) => c.is_valid(),
            FeeConfig::PercentageFee(c) => c.is_valid(),
            FeeConfig::TimeBased(c) => c.is_valid(),
            FeeConfig::TransactionBased(c) => c.is_valid(),
            FeeConfig::Unknown => false,
        }
    }

    pub fn is_active(&self) -> bool {
        return match self {
            FeeConfig::FixedFee(_) => true,
            FeeConfig::PercentageFee(_) => true,
            FeeConfig::TimeBased(c) => c.is_active(),
            FeeConfig::TransactionBased(c) => c.is_active(),
            FeeConfig::Unknown => false,
        }
    }

    pub fn get_fee(&self, original_fee: BalanceOf<T>) -> Result<BalanceOf<T>, Error<T>> {
        return match self {
            FeeConfig::FixedFee(c) => c.get_fee(original_fee),
            FeeConfig::PercentageFee(c) => c.get_fee(original_fee),
            FeeConfig::TimeBased(c) => c.get_fee(original_fee),
            FeeConfig::TransactionBased(c) => c.get_fee(original_fee),
            FeeConfig::Unknown => Ok(original_fee),
        }
    }
}

#[derive(Encode, Decode, MaxEncodedLen, Default, Clone, PartialEq, Debug, Eq, TypeInfo, Copy)]
#[scale_info(skip_type_params(T))]
pub struct FixedFeeConfig<T: Config> {
    pub fee: BalanceOf<T>,
}

impl<T: Config> FixedFeeConfig<T> {
    pub fn is_valid(&self) -> bool {
        return !self.fee.is_zero()
    }

    pub fn is_active(&self) -> bool {
        return true
    }

    pub fn get_fee(&self, original_fee: BalanceOf<T>) -> Result<BalanceOf<T>, Error<T>> {
        if self.is_active() {
            return Ok(self.fee)
        }

        // There is no adjutment to make so return the original fee
        return Ok(original_fee)
    }
}

#[derive(Encode, Decode, MaxEncodedLen, Default, Clone, PartialEq, Debug, Eq, TypeInfo, Copy)]
#[scale_info(skip_type_params(T))]
pub struct PercentageFeeConfig<T: Config> {
    pub percentage: u32,
    _marker: PhantomData<T>,
}

impl<T: Config> PercentageFeeConfig<T> {
    pub fn is_valid(&self) -> bool {
        return !self.percentage.is_zero()
    }

    pub fn is_active(&self) -> bool {
        return true
    }

    pub fn get_fee(&self, original_fee: BalanceOf<T>) -> Result<BalanceOf<T>, Error<T>> {
        if self.is_active() {
            return Ok(Perbill::from_percent(self.percentage) * original_fee)
        }

        // There is no adjutment to make so return the original fee
        return Ok(original_fee)
    }
}

#[derive(Encode, Decode, MaxEncodedLen, Default, Clone, PartialEq, Debug, Eq, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct TimeBasedConfig<T: Config> {
    pub fee_type: FeeType<T>,

    // How many blocks (in block number) this config will be active for.
    pub duration: T::BlockNumber,
    // The last block number. CurrentBlock + duration.
    end_block_number: T::BlockNumber,
}

impl<T: Config> TimeBasedConfig<T> {
    pub fn is_valid(&self) -> bool {
        return !self.end_block_number.is_zero() && self.fee_type != FeeType::Unknown
    }

    pub fn is_active(&self) -> bool {
        return self.end_block_number >= <frame_system::Pallet<T>>::block_number()
    }

    pub fn get_fee(&self, original_fee: BalanceOf<T>) -> Result<BalanceOf<T>, Error<T>> {
        if self.is_active() {
            return calculate_fee::<T>(original_fee, &self.fee_type)
        }

        // There is no adjutment to make so return the original fee
        return Ok(original_fee)
    }

    pub fn set_end_block_number(&mut self, now: T::BlockNumber) -> Result<(), Error<T>> {
        self.end_block_number = now.saturating_add(self.duration);
        Ok(())
    }
}

#[derive(Encode, Decode, MaxEncodedLen, Default, Clone, PartialEq, Debug, Eq, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct TransactionBasedConfig<T: Config> {
    pub fee_type: FeeType<T>,

    // How many transactions this will apply for
    pub count: T::Index,
    // What is the end index (nonce). CurrentIndex + count
    end_count: T::Index,
    account: T::AccountId,
}

impl<T: Config> TransactionBasedConfig<T> {
    pub fn is_valid(&self) -> bool {
        return !self.end_count.is_zero() && self.fee_type != FeeType::Unknown
    }

    pub fn is_active(&self) -> bool {
        return self.end_count >= <frame_system::Pallet<T>>::account(&self.account).nonce
    }

    pub fn get_fee(&self, original_fee: BalanceOf<T>) -> Result<BalanceOf<T>, Error<T>> {
        if self.is_active() {
            return calculate_fee::<T>(original_fee, &self.fee_type)
        }

        // There is no adjutment to make so return the original fee
        return Ok(original_fee)
    }

    pub fn set_fields(&mut self, current_nonce: T::Index, account: T::AccountId) {
        self.account = account;
        self.end_count = current_nonce.saturating_add(self.count);
    }
}

fn calculate_fee<T: Config>(
    original_fee: BalanceOf<T>,
    fee_type: &FeeType<T>,
) -> Result<BalanceOf<T>, Error<T>> {
    return match fee_type {
        FeeType::FixedFee(f) => Ok(f.fee),
        FeeType::PercentageFee(p) => {
            return Ok(Perbill::from_percent(p.percentage) * original_fee)
        },
        _ => Err(Error::InvalidFeeType),
    }
}
