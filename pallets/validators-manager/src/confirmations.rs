use frame_support::log;
use sp_io::hashing::keccak_256;

const PACKED_KEYS_SIZE: usize = 96;

#[cfg(test)]
#[path = "tests/confirmation_tests.rs"]
mod confirmation_tests;