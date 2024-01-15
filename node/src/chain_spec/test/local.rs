use crate::chain_spec::{
    avn_chain_properties, constants::*, helpers::*, ChainType, EthPublicKey, Extensions,
};

use crate::chain_spec::test::{avn_test_runtime_genesis, get_account_id_from_seed, ChainSpec};
use hex_literal::hex;
use sp_core::{ecdsa, sr25519, ByteArray, H160};

pub fn avn_garde_local_config() -> ChainSpec {
    ChainSpec::from_genesis(
        // Name
        "AvN Garde Local Parachain",
        // ID
        "avn_garde_local",
        ChainType::Local,
        move || {
            avn_test_runtime_genesis(
                // initial collators.
                vec![get_authority_keys_from_seed("Eve"), get_authority_keys_from_seed("Ferdie")],
                vec![
                    (get_account_id_from_seed::<sr25519::Public>("Alice"), AVT_ENDOWMENT),
                    (get_account_id_from_seed::<sr25519::Public>("Bob"), AVT_ENDOWMENT),
                    (get_account_id_from_seed::<sr25519::Public>("Charlie"), AVT_ENDOWMENT),
                    (get_account_id_from_seed::<sr25519::Public>("Dave"), AVT_ENDOWMENT),
                    (get_account_id_from_seed::<sr25519::Public>("Eve"), AVT_ENDOWMENT),
                    (get_account_id_from_seed::<sr25519::Public>("Ferdie"), AVT_ENDOWMENT),
                    (get_account_id_from_seed::<sr25519::Public>("Alice//stash"), AVT_ENDOWMENT),
                    (get_account_id_from_seed::<sr25519::Public>("Bob//stash"), AVT_ENDOWMENT),
                    (get_account_id_from_seed::<sr25519::Public>("Charlie//stash"), AVT_ENDOWMENT),
                    (get_account_id_from_seed::<sr25519::Public>("Dave//stash"), AVT_ENDOWMENT),
                    (get_account_id_from_seed::<sr25519::Public>("Eve//stash"), AVT_ENDOWMENT),
                    (get_account_id_from_seed::<sr25519::Public>("Ferdie//stash"), AVT_ENDOWMENT),
                    (get_account_id_from_seed::<sr25519::Public>("Bank"), AVT_ENDOWMENT),
                    (get_account_id_from_seed::<sr25519::Public>("gateway-relayer"), AVT_ENDOWMENT),
                    (
                        get_account_id_from_seed::<sr25519::Public>("nft-marketplace-relayer"),
                        AVT_ENDOWMENT,
                    ),
                    (
                        get_account_id_from_seed::<sr25519::Public>("onboarding-relayer"),
                        AVT_ENDOWMENT,
                    ),
                    (get_account_id_from_seed::<sr25519::Public>("test-account"), AVT_ENDOWMENT),
                ],
                2000.into(),
                // SUDO account
                get_account_id_from_seed::<sr25519::Public>("Ferdie"),
                // AVT contract
                H160(hex!("93ba86eCfDDD9CaAAc29bE83aCE5A3188aC47730")),
                // AVN contract
                H160(hex!("9d6394ea67D297b4Fc777f719F82Ae1F1fc06383")),
                vec![],
                avn_garde_local_ethereum_public_keys(),
                vec![],
                SMALL_EVENT_CHALLENGE_PERIOD,
                HALF_HOUR_SCHEDULE_PERIOD,
                SMALL_VOTING_PERIOD,
            )
        },
        // Bootnodes
        Vec::new(),
        // Telemetry
        None,
        // Protocol ID
        Some("avn_garde_local"),
        // Fork ID
        None,
        // Properties
        avn_chain_properties(),
        // Extensions
        Extensions {
            relay_chain: "rococo-local".into(), // You MUST set this to the correct network!
            para_id: 2000,
        },
    )
}

fn avn_garde_local_ethereum_public_keys() -> Vec<EthPublicKey> {
    return vec![
        // 0x2930e4cE246546597e87FeBaA2B1cde341FD0944
        ecdsa::Public::from_slice(&hex![
            "02226029a1ab8df6f4a4b809c7a2f27ea41cb31b3461ccee650e2cbd362dfac527"
        ])
        .unwrap(),
        // 0xE8493F12e27b961572F7c91E4FaA950c364121a9
        ecdsa::Public::from_slice(&hex![
            "032b411c7a438c79788551ad5ef31e6f6a1ed1fa16ba3c699188f3ea2f43ad271a"
        ])
        .unwrap(),
        // 0x80750Ebf9eB42BccF4C90edB5a0a6C23a6cECaFe
        ecdsa::Public::from_slice(&hex![
            "038d8366fb6b59b0fa55bd92d04b913938cd3fdaa94574ea70058a2c89531b7a85"
        ])
        .unwrap(),
        // 0x6794816e3C4Eba995bBfeb4f9CF7D05fb5e4c2dC
        ecdsa::Public::from_slice(&hex![
            "036c1aff807f50481dcffc691fcfc4b8aecbd34e07b50c6ef6a77abe73b5ba2b01"
        ])
        .unwrap(),
    ]
}
