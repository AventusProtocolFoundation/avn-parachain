use crate::{BalanceOf, Config, Error};
use codec::{Decode, Encode, MaxEncodedLen};
use sp_runtime::{scale_info::TypeInfo, traits::Zero, Perbill, Saturating};
use sp_std::{fmt::Debug};

#[derive(Encode, Decode, MaxEncodedLen, Clone, PartialEq, Eq, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub enum FeeAdjustmentConfig<T: Config> {
    FixedFee(FixedFeeConfig<T>),
    PercentageFee(PercentageFeeConfig<T>),
    None,
}

impl<T: Config> Debug for FeeAdjustmentConfig<T> {
    fn fmt(&self, f: &mut sp_std::fmt::Formatter<'_>) -> sp_std::fmt::Result {
        match self {
            Self::FixedFee(c) => {
                write!(f, "Fixed fee[{:?}]", c.fee)
            },
            Self::PercentageFee(c) => {
                write!(f, "Percentage fee[{}]", c.percentage)
            },
            Self::None => {
                write!(f, "Fee config unknown")
            },
        }
    }
}

impl<T: Config> Default for FeeAdjustmentConfig<T> {
    fn default() -> Self {
        FeeAdjustmentConfig::None
    }
}

impl<T: Config> FeeAdjustmentConfig<T> {
    pub fn is_valid(&self) -> bool {
        return match self {
            FeeAdjustmentConfig::FixedFee(c) => c.is_valid(),
            FeeAdjustmentConfig::PercentageFee(c) => c.is_valid(),
            FeeAdjustmentConfig::None => false,
        }
    }

    pub fn is_active(&self) -> bool {
        return match self {
            FeeAdjustmentConfig::FixedFee(_) => true,
            FeeAdjustmentConfig::PercentageFee(_) => true,
            FeeAdjustmentConfig::None => false,
        }
    }

    pub fn get_fee(&self, original_fee: BalanceOf<T>) -> Result<BalanceOf<T>, Error<T>> {
        return match self {
            FeeAdjustmentConfig::FixedFee(c) => c.get_fee(original_fee),
            FeeAdjustmentConfig::PercentageFee(c) => c.get_fee(original_fee),
            FeeAdjustmentConfig::None => Ok(original_fee),
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
    #[codec(skip)]
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

        // There is no adjustment to make so return the original fee
        return Ok(original_fee)
    }
}

