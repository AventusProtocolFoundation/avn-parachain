use crate::{Config, BalanceOf, Error};
use codec::{Decode, Encode};
use sp_runtime::{ Perbill, scale_info::TypeInfo, traits::Zero };
use sp_std::{marker::PhantomData};

#[derive(Encode, Decode, Clone, PartialEq, Debug, Eq, TypeInfo, Copy)]
pub enum FeeType<T: Config> {
    FixedFee(FixedFeeConfig<T>),
    PercentageFee(PercentageFeeConfig<T>),
    Unknown,
}

impl<T: Config> Default for FeeType<T> {
    fn default() -> Self {
        FeeType::Unknown
    }
}

#[derive(Encode, Decode, Clone, PartialEq, Debug, Eq, TypeInfo)]
pub enum FeeConfig<T: Config> {
    FixedFee(FixedFeeConfig<T>),
    PercentageFee(PercentageFeeConfig<T>),
    TimeBased(TimeBasedConfig<T>),
    TransactionBased(TransactionBasedConfig<T>),
    Unknown,
}

impl<T: Config> FeeConfig<T> {
    pub fn is_valid(&self) -> bool {
        return match self {
            FeeConfig::FixedFee(c) => c.is_valid(),
            FeeConfig::PercentageFee(c) => c.is_valid(),
            FeeConfig::TimeBased(c) => c.is_valid(),
            FeeConfig::TransactionBased(c) => c.is_valid(),
            FeeConfig::Unknown => false,
        };
    }

    pub fn is_active(&self) -> bool {
        return match self {
            FeeConfig::FixedFee(_) => true,
            FeeConfig::PercentageFee(_) => true,
            FeeConfig::TimeBased(c) => c.is_active(),
            FeeConfig::TransactionBased(c) => c.is_active(),
            FeeConfig::Unknown => false,
        };
    }

    pub fn get_fee(&self, original_fee: BalanceOf<T>) -> Result<BalanceOf<T>, Error<T>> {
        return match self {
            FeeConfig::FixedFee(c) => c.get_fee(original_fee),
            FeeConfig::PercentageFee(c) => c.get_fee(original_fee),
            FeeConfig::TimeBased(c) => c.get_fee(original_fee),
            FeeConfig::TransactionBased(c) => c.get_fee(original_fee),
            FeeConfig::Unknown => Ok(original_fee),
        };
    }
}


impl<T: Config> Default for FeeConfig<T> {
    fn default() -> Self {
        FeeConfig::Unknown
    }
}

#[derive(Encode, Decode, Default, Clone, PartialEq, Debug, Eq, TypeInfo, Copy)]
pub struct FixedFeeConfig<T: Config> {
    pub fee: BalanceOf<T>,
}

impl<T: Config> FixedFeeConfig<T> {
    pub fn is_valid(&self) -> bool {
        return !self.fee.is_zero();
    }

    pub fn is_active(&self) -> bool {
        return true;
    }

    pub fn get_fee(&self, original_fee: BalanceOf<T>) -> Result<BalanceOf<T>, Error<T>> {
        if self.is_active() {
            return Ok(self.fee);
        }

        // There is no adjutment to make so return the original fee
        return Ok(original_fee);
    }
}


#[derive(Encode, Decode, Default, Clone, PartialEq, Debug, Eq, TypeInfo, Copy)]
pub struct PercentageFeeConfig<T: Config> {
    pub percentage: u32,
    _marker: PhantomData<T>
}

impl<T: Config> PercentageFeeConfig<T> {
    pub fn is_valid(&self) -> bool {
        return !self.percentage.is_zero();
    }

    pub fn is_active(&self) -> bool {
        return true;
    }

    pub fn get_fee(&self, original_fee: BalanceOf<T>) -> Result<BalanceOf<T>, Error<T>> {
        if self.is_active() {
            return Ok(Perbill::from_percent(self.percentage) * original_fee);
        }

        // There is no adjutment to make so return the original fee
        return Ok(original_fee);
    }
}


#[derive(Encode, Decode, Default, Clone, PartialEq, Debug, Eq, TypeInfo)]
pub struct TimeBasedConfig<T: Config> {
    pub end_block_number: T::BlockNumber,
    pub fee_type: FeeType<T>,
}

impl<T: Config> TimeBasedConfig<T> {
    pub fn is_valid(&self) -> bool {
        return !self.end_block_number.is_zero() &&
            self.fee_type != FeeType::Unknown
    }

    pub fn is_active(&self) -> bool {
        return self.end_block_number >= <frame_system::Pallet<T>>::block_number();
    }

    pub fn get_fee(&self, original_fee: BalanceOf<T>) -> Result<BalanceOf<T>, Error<T>> {
        if self.is_active() {
            return calculate_fee::<T>(original_fee, &self.fee_type);
        }

        // There is no adjutment to make so return the original fee
        return Ok(original_fee);
    }
}

#[derive(Encode, Decode, Default, Clone, PartialEq, Debug, Eq, TypeInfo)]
pub struct TransactionBasedConfig<T: Config> {
    pub account: T::AccountId,
    pub end_count: T::Index,
    pub fee_type: FeeType<T>,
}

impl<T: Config> TransactionBasedConfig<T> {
    pub fn is_valid(&self) -> bool {
        return !self.end_count.is_zero() &&
            self.fee_type != FeeType::Unknown
    }

    pub fn is_active(&self) -> bool {
        return self.end_count >= <frame_system::Pallet<T>>::account(&self.account).nonce;
    }

    pub fn get_fee(&self, original_fee: BalanceOf<T>) -> Result<BalanceOf<T>, Error<T>> {
        if self.is_active() {
            return calculate_fee::<T>(original_fee, &self.fee_type);
        }

        // There is no adjutment to make so return the original fee
        return Ok(original_fee);
    }
}

fn calculate_fee<T: Config>(original_fee: BalanceOf<T>, fee_type: &FeeType<T>) -> Result<BalanceOf<T>, Error<T>> {
    return match fee_type {
        FeeType::FixedFee(f) => Ok(f.fee),
        FeeType::PercentageFee(p) => {
            return Ok(Perbill::from_percent(p.percentage) * original_fee);
        },
        _ => Err(Error::InvalidFeeType)
    }
}
