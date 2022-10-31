#[cfg(test)]
use super::*;
use sp_core::offchain::{testing, OffchainDbExt as OffchainExt};
use sp_io::TestExternalities;

fn generate_db_entry_name(data_name_seed: u64) -> PersistentId {
    let mut persistent_id = b"locked_data_:".to_vec();
    persistent_id.extend_from_slice(&mut data_name_seed.encode());
    persistent_id
}

fn create_db_entry_data(
    operation: OcwOperationExpiration,
) -> (u64, u64, PersistentId, LocalDBEntry<u64, u64>) {
    let starting_block = 35;
    let data_to_store: u64 = 1000;
    let persistent_id = generate_db_entry_name(10);
    (
        starting_block,
        data_to_store,
        persistent_id.clone(),
        LocalDBEntry::<u64, u64>::new(starting_block, operation, data_to_store, persistent_id),
    )
}

#[test]
fn create_local_db_entry_with_new_slow_success() {
    let (starting_block, data_to_store, persistent_id, new_db_entry) =
        create_db_entry_data(OcwOperationExpiration::Slow);

    assert_eq!(
        new_db_entry.expiry,
        starting_block + u64::from(OcwOperationExpiration::Slow.block_delay())
    );
    assert_eq!(new_db_entry.data, data_to_store);
    assert_eq!(new_db_entry.persistent_id, persistent_id);
}

#[test]
fn create_local_db_entry_with_new_fast_success() {
    let (starting_block, data_to_store, persistent_id, new_db_entry) =
        create_db_entry_data(OcwOperationExpiration::Fast);

    assert_eq!(
        new_db_entry.expiry,
        starting_block + u64::from(OcwOperationExpiration::Fast.block_delay())
    );
    assert_eq!(new_db_entry.data, data_to_store);
    assert_eq!(new_db_entry.persistent_id, persistent_id);
}

#[test]
fn create_local_db_entry_with_new_custom_success() {
    let custom_block_expiry: u32 = 500;
    let (starting_block, data_to_store, persistent_id, new_db_entry) =
        create_db_entry_data(OcwOperationExpiration::Custom(custom_block_expiry));

    assert_eq!(new_db_entry.expiry, starting_block + u64::from(custom_block_expiry));
    assert_eq!(new_db_entry.data, data_to_store);
    assert_eq!(new_db_entry.persistent_id, persistent_id);
}

#[test]
fn unique_data_added_to_storage_and_expiry_list_success() {
    // TODO [TYPE: test refactoring][PRI: low]: consider wrapping these externalities in a centralized builder
    let (offchain, _state) = testing::TestOffchainExt::new();
    let mut t = TestExternalities::default();
    t.register_extension(OffchainExt::new(offchain));

    t.execute_with(|| {
        let (starting_block, _, persistent_id, db_entry) =
            create_db_entry_data(OcwOperationExpiration::Slow);
        assert!(set_lock_with_expiry(starting_block, OcwOperationExpiration::Slow, persistent_id)
            .is_ok());

        assert!(is_locked(&db_entry.persistent_id));

        let expiry_list_for_block = get_expiring_list_for_block(&db_entry.expiry);
        assert!(expiry_list_for_block.is_some());
        assert_eq!(expiry_list_for_block, Some(vec![db_entry.persistent_id]));
    });
}

#[test]
fn unique_data_added_twice_to_storage_fails() {
    let (offchain, _state) = testing::TestOffchainExt::new();
    let mut t = TestExternalities::default();
    t.register_extension(OffchainExt::new(offchain));

    t.execute_with(|| {
        let (starting_block, _, persistent_id, _) =
            create_db_entry_data(OcwOperationExpiration::Slow);

        assert!(set_lock_with_expiry(
            starting_block,
            OcwOperationExpiration::Slow,
            persistent_id.clone()
        )
        .is_ok());
        assert!(set_lock_with_expiry(
            starting_block,
            OcwOperationExpiration::Slow,
            persistent_id.clone()
        )
        .is_err());

        assert!(is_locked(&persistent_id));
    });
}

#[test]
fn expired_items_are_removed_successfully() {
    let (offchain, _state) = testing::TestOffchainExt::new();
    let mut t = TestExternalities::default();
    t.register_extension(OffchainExt::new(offchain));

    t.execute_with(|| {
        let (starting_block, _, first_id, db_entry) =
            create_db_entry_data(OcwOperationExpiration::Slow);
        let second_id = generate_db_entry_name(100);

        assert!(set_lock_with_expiry(
            starting_block,
            OcwOperationExpiration::Slow,
            first_id.clone()
        )
        .is_ok());
        assert!(set_lock_with_expiry(
            starting_block,
            OcwOperationExpiration::Slow,
            second_id.clone()
        )
        .is_ok());

        cleanup_expired_entries(&db_entry.expiry);

        assert!(!is_locked(&first_id));
        assert!(!is_locked(&second_id));
        assert!(get_expiring_list_for_block(&db_entry.expiry).is_none());
    });
}

#[test]
fn expiry_removes_previous_blocks_as_well_successfully() {
    let (offchain, _state) = testing::TestOffchainExt::new();
    let mut t = TestExternalities::default();
    t.register_extension(OffchainExt::new(offchain));

    t.execute_with(|| {
        let (starting_block, _, first_id, db_entry) =
            create_db_entry_data(OcwOperationExpiration::Slow);
        let second_id = generate_db_entry_name(100);
        let earlier_block = starting_block - 1;

        assert!(set_lock_with_expiry(
            earlier_block,
            OcwOperationExpiration::Slow,
            second_id.clone()
        )
        .is_ok());
        assert!(set_lock_with_expiry(
            starting_block,
            OcwOperationExpiration::Slow,
            first_id.clone()
        )
        .is_ok());

        cleanup_expired_entries(&db_entry.expiry);

        assert!(!is_locked(&first_id));
        assert!(!is_locked(&second_id));
        assert!(get_expiring_list_for_block(&db_entry.expiry).is_none());
    });
}

mod storage_locks {
    use super::*;

    struct Context {
        pub start_block: u64,
        pub operation: OcwOperationExpiration,
        pub id: PersistentId,
        pub other_id: PersistentId,
    }

    impl Default for Context {
        fn default() -> Self {
            Context {
                start_block: 35,
                operation: OcwOperationExpiration::Slow,
                id: generate_db_entry_name(10),
                other_id: generate_db_entry_name(20),
            }
        }
    }

    impl Context {
        fn insert_slow_lock(&self) {
            assert!(set_lock_with_expiry(
                self.start_block,
                self.operation.clone(),
                self.id.clone()
            )
            .is_ok());
        }
        fn insert_other_slow_lock(&self) {
            assert!(set_lock_with_expiry(
                self.start_block,
                self.operation.clone(),
                self.other_id.clone()
            )
            .is_ok());
        }
    }

    mod delete_lock {
        use super::*;

        mod removes_lock {
            use super::*;

            #[test]
            fn when_lock_exists() {
                let (offchain, _state) = testing::TestOffchainExt::new();
                let mut t = TestExternalities::default();
                t.register_extension(OffchainExt::new(offchain));

                t.execute_with(|| {
                    let context = Context::default();
                    context.insert_slow_lock();
                    assert!(remove_storage_lock(
                        context.start_block,
                        context.operation,
                        context.id.clone()
                    )
                    .is_ok());
                    assert!(!is_locked(&context.id));
                });
            }

            #[test]
            fn and_not_other() {
                let (offchain, _state) = testing::TestOffchainExt::new();
                let mut t = TestExternalities::default();
                t.register_extension(OffchainExt::new(offchain));

                t.execute_with(|| {
                    let context = Context::default();
                    context.insert_slow_lock();
                    context.insert_other_slow_lock();
                    assert!(remove_storage_lock(
                        context.start_block,
                        context.operation,
                        context.id.clone()
                    )
                    .is_ok());
                    assert!(!is_locked(&context.id));
                    assert!(is_locked(&context.other_id));
                });
            }
        }

        mod fails_when {
            use super::*;

            #[test]
            fn lock_does_not_exist() {
                let (offchain, _state) = testing::TestOffchainExt::new();
                let mut t = TestExternalities::default();
                t.register_extension(OffchainExt::new(offchain));

                t.execute_with(|| {
                    let context = Context::default();
                    assert!(remove_storage_lock(
                        context.start_block,
                        context.operation,
                        context.id
                    )
                    .is_err());
                });
            }
        }
    }
}
