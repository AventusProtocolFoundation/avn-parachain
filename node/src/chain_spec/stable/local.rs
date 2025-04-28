use crate::chain_spec::{
    avn_chain_properties, constants::*, ChainSpec, ChainType, EthPublicKey, Extensions,
};

use avn_parachain_runtime::{self as avn_runtime};

use crate::chain_spec::stable::{
    get_account_id_from_seed, get_account_id_from_seed_no_derivation, get_authority_keys_from_seed,
    testnet_genesis,
};
use hex_literal::hex;
use sp_core::{ecdsa, sr25519, ByteArray, H160};

pub fn development_config() -> ChainSpec {
    let dev_rococo_parachain_id: u32 = 2060;
    ChainSpec::from_genesis(
        // Name
        "AvN Local Development Parachain",
        // ID
        "dev",
        ChainType::Development,
        move || {
            testnet_genesis(
                // initial collators.
                vec![
                    get_authority_keys_from_seed("Alice//stash"),
                    get_authority_keys_from_seed("Ferdie"),
                ],
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
                    // Use in avn-proxy benchmarks
                    (get_account_id_from_seed_no_derivation::<sr25519::Public>("kiss mule sheriff twice make bike twice improve rate quote draw enough"), AVT_ENDOWMENT),
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
                dev_rococo_parachain_id.into(),
                // SUDO account
                get_account_id_from_seed::<sr25519::Public>("Ferdie"),
                // AVT contract
                H160(hex!("93ba86eCfDDD9CaAAc29bE83aCE5A3188aC47730")),
                // AVN contract
                H160(hex!("9d6394ea67D297b4Fc777f719F82Ae1F1fc06383")),
                vec![],
                dev_ethereum_public_keys(),
                vec![],
                SMALL_EVENT_CHALLENGE_PERIOD,
                HALF_HOUR_SCHEDULE_PERIOD,
                SMALL_VOTING_PERIOD,
                // Non AVT token address
                Some(H160(hex!("ea5da4fd16cc61ffc4235874d6ff05216e3e038e"))),
            )
        },
        Vec::new(),
        None,
        Some("avn-dev"),
        None,
        avn_chain_properties(),
        Extensions {
            relay_chain: "rococo-local".into(), // You MUST set this to the correct network!
            para_id: dev_rococo_parachain_id,
        },
        avn_runtime::WASM_BINARY.expect("WASM binary was not build, please build it!"),
    )
}

pub fn local_testnet_config() -> ChainSpec {
    ChainSpec::from_genesis(
        // Name
        "AvN Local Parachain",
        // ID
        "local_testnet",
        ChainType::Local,
        move || {
            testnet_genesis(
                // initial collators.
                vec![
                    get_authority_keys_from_seed("Eve"),
                    get_authority_keys_from_seed("Ferdie"),
                    get_authority_keys_from_seed("Dave"),
                    get_authority_keys_from_seed("Charlie"),
                ],
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
                    // Use in avn-proxy benchmarks
                    (get_account_id_from_seed_no_derivation::<sr25519::Public>("kiss mule sheriff twice make bike twice improve rate quote draw enough"), AVT_ENDOWMENT),
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
                dev_ethereum_public_keys(),
                vec![],
                SMALL_EVENT_CHALLENGE_PERIOD,
                HALF_HOUR_SCHEDULE_PERIOD,
                SMALL_VOTING_PERIOD,
                // Non AVT token address
                Some(H160(hex!("ea5da4fd16cc61ffc4235874d6ff05216e3e038e"))),
            )
        },
        // Bootnodes
        Vec::new(),
        // Telemetry
        None,
        // Protocol ID
        Some("avn-local"),
        // Fork ID
        None,
        // Properties
        avn_chain_properties(),
        // Extensions
        Extensions {
            relay_chain: "rococo-local".into(), // You MUST set this to the correct network!
            para_id: 2000,
        },
        avn_runtime::WASM_BINARY.expect("WASM binary was not build, please build it!"),
    )
}

fn dev_ethereum_public_keys() -> Vec<EthPublicKey> {
    /*
        The following test public keys are generated with 12 word mnemonic:
        decade mask flag steak negative eagle crunch sea evoke rack drive print

        Derivation			Address										Public key																Private key
        m/44'/60'/0'/0/0	0xFB509cFaCE208C271f9fBEAC5ac39cEC245e3587	0x03471b4c1012dddf4d494c506a098c7b1b719b20bbb177b1174f2166f953c29503	0x35f071b8e86c710b71c5cdbf95a9a146e152e64e7b13e3d15d278cab5f1ec517
        m/44'/60'/0'/0/1	0x5ebf86c1749bbF45Bf0dAb36B1E4D81836e03953	0x0292a73ad9488b934fd04cb31a0f50634841f7105a5b4a8538e4bfa06aa477bed6	0xbaf9b03f2e80afc21ffe0bbb908d25cc8e99be3e60f8485d8a7c8fd813dc8606
        m/44'/60'/0'/0/2	0x97cD8BC20EF2f9bE4eaf2EA52906d71C4aeaa766	0x03c5527886d8e09ad1fededd3231f890685d2d5345385d54181269f80c8926ff8e	0x4f751d57b6595fcfe851774a7dd281b17061bd9d9e4b0551f870013aff6df790
        m/44'/60'/0'/0/3	0xDef5375191e70257ea8cdc1246Dfdd295046b477	0x020e7593c534411f6f0e2fb91340751ada34ee5986f70b300443be17844416b28b	0xfa9bc1b70b739012ba34d0530dcbd09cbbd5ada78a52ec8d5c7ea30758eb559e
        m/44'/60'/0'/0/4	0xa06964bAC7b3A9cC488241e7419f617e469fBf24	0x02fde5665a2cb42863fb312fb527f2b02110997fc6865df583ca4324be137b7894	0x09a7205005ad40afa7c590b5fa6d26fabe23f07790f758ac06a7c9fbc56d06ae
        m/44'/60'/0'/0/5	0x54A26b5082F3b26d5472E080c54372aB5c1D867F	0x031f8860a4f05ec62077a97d37af60f0229b775b98946efcb92998522abefc1b6c	0xfaf4f3166d415b7f356655434d808962442471e98e11d973bd5270a71e39a13b
    */
    return vec![
        ecdsa::Public::from_slice(&hex![
            "03471b4c1012dddf4d494c506a098c7b1b719b20bbb177b1174f2166f953c29503"
        ])
        .unwrap(),
        ecdsa::Public::from_slice(&hex![
            "0292a73ad9488b934fd04cb31a0f50634841f7105a5b4a8538e4bfa06aa477bed6"
        ])
        .unwrap(),
        ecdsa::Public::from_slice(&hex![
            "03c5527886d8e09ad1fededd3231f890685d2d5345385d54181269f80c8926ff8e"
        ])
        .unwrap(),
        ecdsa::Public::from_slice(&hex![
            "020e7593c534411f6f0e2fb91340751ada34ee5986f70b300443be17844416b28b"
        ])
        .unwrap(),
        ecdsa::Public::from_slice(&hex![
            "02fde5665a2cb42863fb312fb527f2b02110997fc6865df583ca4324be137b7894"
        ])
        .unwrap(),
        ecdsa::Public::from_slice(&hex![
            "031f8860a4f05ec62077a97d37af60f0229b775b98946efcb92998522abefc1b6c"
        ])
        .unwrap(),
    ]
}
