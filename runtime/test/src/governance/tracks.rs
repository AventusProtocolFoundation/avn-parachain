// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot. If not, see <http://www.gnu.org/licenses/>.

//! Track configurations for governance.

use super::*;

const fn percent(x: i32) -> sp_runtime::FixedI64 {
    sp_runtime::FixedI64::from_rational(x as u128, 100)
}
use pallet_referenda::Curve;
const APP_ROOT: Curve = Curve::make_reciprocal(1, 2, percent(80), percent(50), percent(100));
const SUP_ROOT: Curve = Curve::make_linear(1, 2, percent(0), percent(50));
const APP_GENERAL_ADMIN: Curve =
    Curve::make_reciprocal(1, 2, percent(80), percent(50), percent(100));
const SUP_GENERAL_ADMIN: Curve = Curve::make_reciprocal(1, 2, percent(10), percent(0), percent(50));
const APP_REFERENDUM_CANCELLER: Curve = Curve::make_linear(17, 28, percent(50), percent(100));
const SUP_REFERENDUM_CANCELLER: Curve =
    Curve::make_reciprocal(12, 2, percent(1), percent(0), percent(50));
const APP_REFERENDUM_KILLER: Curve = Curve::make_linear(17, 28, percent(50), percent(100));
const SUP_REFERENDUM_KILLER: Curve =
    Curve::make_reciprocal(12, 2, percent(1), percent(0), percent(50));
const APP_WHITELISTED_CALLER: Curve =
    Curve::make_reciprocal(16, 2 * 24, percent(96), percent(50), percent(100));
const SUP_WHITELISTED_CALLER: Curve =
    Curve::make_reciprocal(1, 2, percent(20), percent(5), percent(50));

const APP_SMALL_SPENDER: Curve = Curve::make_linear(17, 28, percent(50), percent(100));
const SUP_SMALL_SPENDER: Curve =
    Curve::make_reciprocal(12, 28, percent(1), percent(0), percent(50));
const APP_MEDIUM_SPENDER: Curve = Curve::make_linear(23, 28, percent(50), percent(100));
const SUP_MEDIUM_SPENDER: Curve =
    Curve::make_reciprocal(16, 28, percent(1), percent(0), percent(50));
const APP_BIG_SPENDER: Curve = Curve::make_linear(28, 28, percent(50), percent(100));
const SUP_BIG_SPENDER: Curve = Curve::make_reciprocal(20, 28, percent(1), percent(0), percent(50));

const TRACKS_DATA: [(u16, pallet_referenda::TrackInfo<Balance, BlockNumber>); 8] = [
    (
        0,
        pallet_referenda::TrackInfo {
            name: "root",
            max_deciding: 1,
            decision_deposit: 1 * AVT,
            prepare_period: 1 * MINUTES,
            decision_period: 1 * MINUTES,
            confirm_period: 1 * MINUTES,
            min_enactment_period: 1 * MINUTES,
            min_approval: APP_ROOT,
            min_support: SUP_ROOT,
        },
    ),
    (
        1,
        pallet_referenda::TrackInfo {
            name: "whitelisted_caller",
            max_deciding: 3,
            decision_deposit: 1 * AVT,
            prepare_period: 1 * MINUTES,
            decision_period: 1 * MINUTES,
            confirm_period: 1 * MINUTES,
            min_enactment_period: 1 * MINUTES,
            min_approval: APP_WHITELISTED_CALLER,
            min_support: SUP_WHITELISTED_CALLER,
        },
    ),
    (
        2,
        pallet_referenda::TrackInfo {
            name: "general_admin",
            max_deciding: 3,
            decision_deposit: 1 * AVT,
            prepare_period: 1 * MINUTES,
            decision_period: 1 * MINUTES,
            confirm_period: 1 * MINUTES,
            min_enactment_period: 1 * MINUTES,
            min_approval: APP_GENERAL_ADMIN,
            min_support: SUP_GENERAL_ADMIN,
        },
    ),
    (
        3,
        pallet_referenda::TrackInfo {
            name: "referendum_canceller",
            max_deciding: 3,
            decision_deposit: 1 * AVT,
            prepare_period: 1 * MINUTES,
            decision_period: 1 * MINUTES,
            confirm_period: 1 * MINUTES,
            min_enactment_period: 1 * MINUTES,
            min_approval: APP_REFERENDUM_CANCELLER,
            min_support: SUP_REFERENDUM_CANCELLER,
        },
    ),
    (
        4,
        pallet_referenda::TrackInfo {
            name: "referendum_killer",
            max_deciding: 3,
            decision_deposit: 1 * AVT,
            prepare_period: 1 * MINUTES,
            decision_period: 1 * MINUTES,
            confirm_period: 1 * MINUTES,
            min_enactment_period: 1 * MINUTES,
            min_approval: APP_REFERENDUM_KILLER,
            min_support: SUP_REFERENDUM_KILLER,
        },
    ),
    (
        5,
        pallet_referenda::TrackInfo {
            name: "small_spender",
            max_deciding: 5,
            decision_deposit: 1 * AVT,
            prepare_period: 1 * MINUTES,
            decision_period: 2 * MINUTES,
            confirm_period: 1 * MINUTES,
            min_enactment_period: 1 * MINUTES,
            min_approval: APP_SMALL_SPENDER,
            min_support: SUP_SMALL_SPENDER,
        },
    ),
    (
        6,
        pallet_referenda::TrackInfo {
            name: "medium_spender",
            max_deciding: 5,
            decision_deposit: 2 * AVT,
            prepare_period: 1 * MINUTES,
            decision_period: 2 * MINUTES,
            confirm_period: 1 * MINUTES,
            min_enactment_period: 1 * MINUTES,
            min_approval: APP_MEDIUM_SPENDER,
            min_support: SUP_MEDIUM_SPENDER,
        },
    ),
    (
        7,
        pallet_referenda::TrackInfo {
            name: "big_spender",
            max_deciding: 5,
            decision_deposit: 4 * AVT,
            prepare_period: 1 * MINUTES,
            decision_period: 2 * MINUTES,
            confirm_period: 1 * MINUTES,
            min_enactment_period: 1 * MINUTES,
            min_approval: APP_BIG_SPENDER,
            min_support: SUP_BIG_SPENDER,
        },
    ),
];

pub struct TracksInfo;
impl pallet_referenda::TracksInfo<Balance, BlockNumber> for TracksInfo {
    type Id = u16;
    type RuntimeOrigin = <RuntimeOrigin as frame_support::traits::OriginTrait>::PalletsOrigin;
    fn tracks() -> &'static [(Self::Id, pallet_referenda::TrackInfo<Balance, BlockNumber>)] {
        &TRACKS_DATA[..]
    }
    fn track_for(id: &Self::RuntimeOrigin) -> Result<Self::Id, ()> {
        if let Ok(system_origin) = frame_system::RawOrigin::try_from(id.clone()) {
            match system_origin {
                frame_system::RawOrigin::Root => Ok(0),
                _ => Err(()),
            }
        } else if let Ok(custom_origin) = origins::Origin::try_from(id.clone()) {
            match custom_origin {
                origins::Origin::WhitelistedCaller => Ok(1),
                // General admin
                origins::Origin::GeneralAdmin => Ok(2),
                // Referendum admins
                origins::Origin::ReferendumCanceller => Ok(3),
                origins::Origin::ReferendumKiller => Ok(4),
                origins::Origin::SmallSpender => Ok(5),
                origins::Origin::MediumSpender => Ok(6),
                origins::Origin::BigSpender => Ok(7),
            }
        } else {
            Err(())
        }
    }
}
pallet_referenda::impl_tracksinfo_get!(TracksInfo, Balance, BlockNumber);
