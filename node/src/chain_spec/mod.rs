mod stable;
pub use stable::*;

#[cfg(feature = "avn-test-runtime")]
mod test;

#[cfg(feature = "avn-test-runtime")]
pub use test::{avn_garde_local_config, avn_garde_staging_config};

use sc_chain_spec::{ChainSpecExtension, ChainSpecGroup};
use serde::{Deserialize, Serialize};
use sp_core::ecdsa;

pub(crate) type EthPublicKey = ecdsa::Public;

pub(crate) use cumulus_primitives_core::ParaId;
pub(crate) use pallet_avn::sr25519::AuthorityId as AvnId;
pub(crate) use pallet_im_online::sr25519::AuthorityId as ImOnlineId;
pub(crate) use sc_service::ChainType;
pub(crate) use sp_authority_discovery::AuthorityId as AuthorityDiscoveryId;
pub(crate) use sp_consensus_aura::sr25519::AuthorityId as AuraId;

pub(crate) mod constants {
    pub use node_primitives::{Balance, BlockNumber};
    pub use runtime_common::constants::{currency::AVT, time::*};

    pub(crate) const SMALL_EVENT_CHALLENGE_PERIOD: BlockNumber = 5 * MINUTES;
    pub(crate) const NORMAL_EVENT_CHALLENGE_PERIOD: BlockNumber = 20 * MINUTES;

    pub(crate) const SMALL_VOTING_PERIOD: BlockNumber = 20 * MINUTES;
    pub(crate) const NORMAL_VOTING_PERIOD: BlockNumber = 30 * MINUTES;

    pub(crate) const HALF_HOUR_SCHEDULE_PERIOD: BlockNumber = 30 * MINUTES;
    pub(crate) const FOUR_HOURS_SCHEDULE_PERIOD: BlockNumber = 4 * HOURS;
    #[cfg(feature = "rococo-spec-build")]
    pub(crate) const EIGHT_HOURS_SCHEDULE_PERIOD: BlockNumber = 8 * HOURS;

    pub(crate) const AVT_ENDOWMENT: Balance = 10_000 * AVT;
    pub(crate) const COLLATOR_DEPOSIT: Balance = 2_000 * AVT;

    pub const QUORUM_FACTOR: u32 = 3;
}

pub(crate) mod helpers {

    use crate::chain_spec::{AuraId, AuthorityDiscoveryId, AvnId, ImOnlineId};
    use node_primitives::{AccountId, Signature};
    use sp_core::{sr25519, Pair, Public};
    use sp_runtime::traits::{IdentifyAccount, Verify};

    pub type AccountPublic = <Signature as Verify>::Signer;

    /// The default XCM version to set in genesis config.
    pub const SAFE_XCM_VERSION: u32 = xcm::prelude::XCM_VERSION;

    /// Helper function to generate a crypto pair from seed
    pub fn get_public_from_seed<TPublic: Public>(seed: &str) -> <TPublic::Pair as Pair>::Public {
        TPublic::Pair::from_string(&format!("//{}", seed), None)
            .expect("static values are valid; qed")
            .public()
    }

    /// Generate collator keys from seed.
    ///
    /// This function's return type must always match the session keys of the chain in tuple format.
    pub fn get_collator_session_keys_from_seed(
        seed: &str,
    ) -> (AuraId, AuthorityDiscoveryId, ImOnlineId, AvnId) {
        (
            get_public_from_seed::<AuraId>(seed),
            get_public_from_seed::<AuthorityDiscoveryId>(seed),
            get_public_from_seed::<ImOnlineId>(seed),
            get_public_from_seed::<AvnId>(seed),
        )
    }

    fn get_seed_with_extra_derivation(seed: &str, extra_derivation: &str) -> String {
        let mut derived_seed = seed.to_owned();
        derived_seed.push_str("//");
        derived_seed.push_str(extra_derivation);

        return derived_seed
    }

    /// Generate collator keys from seed using the name of the key as extra derivation path.
    ///
    /// This function's return type must always match the session keys of the chain in tuple format.
    pub fn get_collator_session_keys_from_seed_with_extra_derivation(
        seed: &str,
    ) -> (AuraId, AuthorityDiscoveryId, ImOnlineId, AvnId) {
        (
            get_public_from_seed::<AuraId>(get_seed_with_extra_derivation(seed, "aura").as_str()),
            get_public_from_seed::<AuthorityDiscoveryId>(
                get_seed_with_extra_derivation(seed, "audi").as_str(),
            ),
            get_public_from_seed::<ImOnlineId>(
                get_seed_with_extra_derivation(seed, "imon").as_str(),
            ),
            get_public_from_seed::<AvnId>(get_seed_with_extra_derivation(seed, "avnk").as_str()),
        )
    }

    /// Helper function to generate an account ID from seed
    pub fn get_account_id_from_seed<TPublic: Public>(seed: &str) -> AccountId
    where
        AccountPublic: From<<TPublic::Pair as Pair>::Public>,
    {
        AccountPublic::from(get_public_from_seed::<TPublic>(seed)).into_account()
    }

    /// Helper function to return the authority keys for a seed
    pub fn get_authority_keys_from_seed(
        seed: &str,
    ) -> (AccountId, AuraId, AuthorityDiscoveryId, ImOnlineId, AvnId) {
        let (aura, audi, imon, avn) = get_collator_session_keys_from_seed(seed);
        return (get_account_id_from_seed::<sr25519::Public>(seed), aura, audi, imon, avn)
    }

    /// Helper function to return the authority keys for a seed with extra derivation
    pub fn get_authority_keys_from_seed_with_derivation(
        seed: &str,
    ) -> (AccountId, AuraId, AuthorityDiscoveryId, ImOnlineId, AvnId) {
        let (aura, audi, imon, avn) =
            get_collator_session_keys_from_seed_with_extra_derivation(seed);
        return (get_account_id_from_seed::<sr25519::Public>(seed), aura, audi, imon, avn)
    }
}

/// The extensions for the [`ChainSpec`].

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ChainSpecGroup, ChainSpecExtension)]
#[serde(deny_unknown_fields)]
pub struct Extensions {
    /// The relay chain of the Parachain.
    pub relay_chain: String,
    /// The id of the Parachain.
    pub para_id: u32,
}

impl Extensions {
    /// Try to get the extension from the given `ChainSpec`.
    pub fn try_get(chain_spec: &dyn sc_service::ChainSpec) -> Option<&Self> {
        sc_chain_spec::get_extension(chain_spec.extensions())
    }
}

/// Sets currency to AVT for an AvN chain
pub(crate) fn avn_chain_properties() -> Option<sc_chain_spec::Properties> {
    // Give your base currency a unit name and decimal places
    let mut properties = sc_chain_spec::Properties::new();
    properties.insert("tokenSymbol".into(), "AVT".into());
    properties.insert("tokenDecimals".into(), 18.into());
    properties.insert("ss58Format".into(), 42.into());
    // TODO: Replace with this when we switch to using custom prefixes
    // properties.insert("ss58Format".into(), 65.into());
    return Some(properties)
}
