use crate::{BalanceOf, Config, Error};
use codec::{Decode, Encode, MaxEncodedLen};
use sp_runtime::{scale_info::TypeInfo, traits::Zero, Perbill, Saturating};
use sp_std::{fmt::Debug};

#[derive(Encode, Decode, MaxEncodedLen, Clone, PartialEq, Eq, TypeInfo, Copy)]
#[scale_info(skip_type_params(T))]
pub enum FeeType<T: Config> {
    FixedFee(FixedFeeConfig<T>),
    PercentageFee(PercentageFeeConfig<T>),
    None,
}
#[derive(Encode, Decode, MaxEncodedLen, Clone, PartialEq, Eq, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub enum FeeAdjustmentConfig<T: Config> {
    FixedFee(FixedFeeConfig<T>),
    PercentageFee(PercentageFeeConfig<T>),
    TimeBased(TimeBasedConfig<T>),
    None,
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
            Self::None => {
                write!(f, "Unknwon fee type")
            },
        }
    }
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
            Self::TimeBased(c) => {
                write!(f, "Time based fee[{:?}, {:?}]", c.end_block_number, c.fee_type)
            },
            Self::None => {
                write!(f, "Fee config unknown")
            },
        }
    }
}

impl<T: Config> Default for FeeType<T> {
    fn default() -> Self {
        FeeType::None
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
            FeeAdjustmentConfig::TimeBased(c) => c.is_valid(),
            FeeAdjustmentConfig::None => false,
        }
    }

    pub fn is_active(&self) -> bool {
        return match self {
            FeeAdjustmentConfig::FixedFee(_) => true,
            FeeAdjustmentConfig::PercentageFee(_) => true,
            FeeAdjustmentConfig::TimeBased(c) => c.is_active(),
            FeeAdjustmentConfig::None => false,
        }
    }

    pub fn get_fee(&self, original_fee: BalanceOf<T>) -> Result<BalanceOf<T>, Error<T>> {
        return match self {
            FeeAdjustmentConfig::FixedFee(c) => c.get_fee(original_fee),
            FeeAdjustmentConfig::PercentageFee(c) => c.get_fee(original_fee),
            FeeAdjustmentConfig::TimeBased(c) => c.get_fee(original_fee),
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

        // There is no adjutment to make so return the original fee
        return Ok(original_fee)
    }
}

#[derive(Encode, Decode, MaxEncodedLen, Default, Clone, PartialEq, Debug, Eq, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct TimeBasedConfig<T: Config> {
    pub fee_type: FeeType<T>,
    end_block_number: T::BlockNumber,
}

impl<T: Config> TimeBasedConfig<T> {
    pub fn is_valid(&self) -> bool {
        return !self.end_block_number.is_zero() && self.fee_type != FeeType::None
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

    pub fn new(fee_type: FeeType<T>, duration: T::BlockNumber) -> Self {
        let end_block_number = <frame_system::Pallet<T>>::block_number().saturating_add(duration);
        return TimeBasedConfig::<T> { fee_type, end_block_number }
    }
}


fn calculate_fee<T: Config>(
    original_fee: BalanceOf<T>,
    fee_type: &FeeType<T>,
) -> Result<BalanceOf<T>, Error<T>> {
    return match fee_type {
        FeeType::FixedFee(f) => Ok(f.fee),
        FeeType::PercentageFee(p) => return Ok(Perbill::from_percent(p.percentage) * original_fee),
        _ => Err(Error::InvalidFeeType),
    }
}
