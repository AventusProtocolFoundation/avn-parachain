// Copyright 2026 Aventus DAO Ltd

//! App-chain reward eligibility: root-set overrides layered over a pluggable base rule.

use crate::{AppChainEligibilityOverrides, AppChainRewardEligibility, Config};
use sp_avn_common::{NodeSerial, RewardPeriodIndex};
use sp_std::marker::PhantomData;

/// [`AppChainRewardEligibility`] implementation that checks [`AppChainEligibilityOverrides`]
/// first and defers to `Fallback` (the base rule) when no override exists for
/// `(node_serial, asset_id)`.
///
/// Implemented as `type AppChainRewardEligibility = OverridableEligibility<Runtime,
/// Base>` where `Base` is `()` (everyone eligible) or a custom rule.
pub struct OverridableEligibility<T, Fallback = ()>(PhantomData<(T, Fallback)>);

impl<T, Fallback> AppChainRewardEligibility<T::AppChainAssetId, T::AccountId>
    for OverridableEligibility<T, Fallback>
where
    T: Config,
    Fallback: AppChainRewardEligibility<T::AppChainAssetId, T::AccountId>,
{
    fn is_eligible(
        asset_id: T::AppChainAssetId,
        node_id: &T::AccountId,
        period: RewardPeriodIndex,
        node_serial: NodeSerial,
    ) -> bool {
        match AppChainEligibilityOverrides::<T>::get(node_serial, asset_id) {
            Some(overriden_eligibility) => overriden_eligibility,
            None => Fallback::is_eligible(asset_id, node_id, period, node_serial),
        }
    }
}
