// Copyright 2024 Aventus Systems (UK) Ltd.
#[cfg(test)]
use super::*;
use sha3::{Digest, Keccak256};
use sp_core::U256;
use sp_std::vec::Vec;

mod end_batch_listing {
    use super::*;

    struct AvtLowerClaimedConfig {
        topic1: Vec<u8>,
        topic2_lower_id: Vec<u8>,
        data: Option<Vec<u8>>,
        bad_topic_short: Vec<u8>,
        bad_topic_long: Vec<u8>,
        bad_data: Option<Vec<u8>>,
    }

    impl AvtLowerClaimedConfig {
        fn setup() -> Self {
            AvtLowerClaimedConfig {
                topic1: vec![1; 32],
                topic2_lower_id: vec![1u8; 32],
                data: None,
                bad_topic_short: vec![10; 16],
                bad_topic_long: vec![10; 64],
                bad_data: Some(vec![2; 1]),
            }
        }
    }

    #[test]
    fn event_signature_should_match() {
        let mut hasher = Keccak256::new();

        hasher.input(b"LogLowerClaimed(uint32)");
        let result = hasher.result();

        assert_eq!(result[..], *ValidEvents::AvtLowerClaimed.signature().as_bytes());
        assert_eq!(
            result[..],
            hex!("9853e4c075911a10a89a0f7a46bac6f8a246c4e9152480d16d86aa6a2391a4f1")[..]
        );
    }

    mod can_successfully_be_parsed {
        use super::*;

        #[test]
        fn when_event_contains_correct_topics_and_data() {
            let config = AvtLowerClaimedConfig::setup();

            let expected_lower_id = u32::from_be_bytes([1u8; 4]);

            let topics = vec![config.topic1, config.topic2_lower_id];
            let result = AvtLowerClaimedData::parse_bytes(config.data, topics);

            assert!(result.is_ok());
            let result = result.unwrap();

            assert_eq!(result.lower_id, expected_lower_id);
        }
    }

    mod fails_parsing_when {
        use super::*;

        #[test]
        fn non_topic_data_is_not_empty() {
            let config = AvtLowerClaimedConfig::setup();

            let topics = vec![config.topic1, config.topic2_lower_id];
            let result = AvtLowerClaimedData::parse_bytes(config.bad_data, topics);

            assert_eq!(result, Err(Error::AvtLowerClaimedEventMissingData));
        }

        #[test]
        fn event_contains_few_topics() {
            let config = AvtLowerClaimedConfig::setup();

            let topics = vec![];
            let result = AvtLowerClaimedData::parse_bytes(config.data, topics);

            assert_eq!(result, Err(Error::AvtLowerClaimedEventWrongTopicCount));
        }

        #[test]
        fn event_contains_too_many_topics() {
            let config = AvtLowerClaimedConfig::setup();

            let topics = vec![config.topic1.clone(), config.topic2_lower_id, config.topic1];
            let result = AvtLowerClaimedData::parse_bytes(config.data, topics);

            assert_eq!(result, Err(Error::AvtLowerClaimedEventWrongTopicCount));
        }

        #[test]
        fn event_contains_short_topic() {
            let config = AvtLowerClaimedConfig::setup();

            let topics = vec![config.topic1, config.bad_topic_short];
            let result = AvtLowerClaimedData::parse_bytes(config.data, topics);

            assert_eq!(result, Err(Error::AvtLowerClaimedEventBadTopicLength));
        }

        #[test]
        fn event_contains_long_topic() {
            let config = AvtLowerClaimedConfig::setup();

            let topics = vec![config.topic1, config.bad_topic_long];
            let result = AvtLowerClaimedData::parse_bytes(config.data, topics);

            assert_eq!(result, Err(Error::AvtLowerClaimedEventBadTopicLength));
        }
    }
}
