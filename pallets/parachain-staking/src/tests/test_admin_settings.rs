#[cfg(test)]
use crate::mock::{
    ExtBuilder, MinNominationPerCollator, ParachainStaking, RuntimeEvent as MetaEvent,
    RuntimeOrigin as Origin, Test, TestAccount,
};
use crate::{
    assert_last_event, AdminSettings, BalanceOf, Delay, Error, Event, MinCollatorStake,
    MinTotalNominatorStake,
};
use frame_support::{assert_noop, assert_ok};
use frame_system::RawOrigin;
use sp_runtime::traits::BadOrigin;
mod delay_admin_setting {
    use super::*;

    #[test]
    fn can_be_updated() {
        ExtBuilder::default().build().execute_with(|| {
            let new_delay_value = <Delay<Test>>::get() - 1;
            let new_delay_setting = AdminSettings::<BalanceOf<Test>>::Delay(new_delay_value);

            assert_ok!(ParachainStaking::set_admin_setting(
                Origin::root(),
                new_delay_setting.clone()
            ));

            assert_eq!(<Delay<Test>>::get(), new_delay_value);
            assert_last_event!(MetaEvent::ParachainStaking(Event::AdminSettingsUpdated {
                value: new_delay_setting
            }));
        });
    }

    #[test]
    fn updating_fails_if_not_signed() {
        ExtBuilder::default().build().execute_with(|| {
            let new_delay_value = <Delay<Test>>::get() - 1;
            let new_delay_setting = AdminSettings::<BalanceOf<Test>>::Delay(new_delay_value);

            assert_noop!(
                ParachainStaking::set_admin_setting(
                    RawOrigin::None.into(),
                    new_delay_setting.clone()
                ),
                BadOrigin
            );
        });
    }

    #[test]
    fn updating_fails_if_sender_not_root() {
        ExtBuilder::default().build().execute_with(|| {
            let non_root_sender = TestAccount::new(20).account_id();
            let new_delay_value = <Delay<Test>>::get() - 1;
            let new_delay_setting = AdminSettings::<BalanceOf<Test>>::Delay(new_delay_value);

            assert_noop!(
                ParachainStaking::set_admin_setting(
                    Origin::signed(non_root_sender),
                    new_delay_setting.clone()
                ),
                BadOrigin
            );
        });
    }

    #[test]
    fn updating_fails_if_delay_is_0() {
        ExtBuilder::default().build().execute_with(|| {
            let bad_delay_value = 0;
            let new_delay_setting = AdminSettings::<BalanceOf<Test>>::Delay(bad_delay_value);

            assert_noop!(
                ParachainStaking::set_admin_setting(Origin::root(), new_delay_setting.clone()),
                Error::<Test>::AdminSettingsValueIsNotValid
            );
        });
    }
}

mod min_nominator_stake_admin_setting {
    use super::*;

    #[test]
    fn can_be_updated() {
        ExtBuilder::default().build().execute_with(|| {
            let new_min_value = <MinTotalNominatorStake<Test>>::get() - 1;
            let new_min_setting =
                AdminSettings::<BalanceOf<Test>>::MinTotalNominatorStake(new_min_value);

            assert_ok!(ParachainStaking::set_admin_setting(
                Origin::root(),
                new_min_setting.clone()
            ));

            assert_eq!(<MinTotalNominatorStake<Test>>::get(), new_min_value);
            assert_last_event!(MetaEvent::ParachainStaking(Event::AdminSettingsUpdated {
                value: new_min_setting
            }));
        });
    }

    #[test]
    fn updating_fails_if_value_is_below_min_nominations() {
        ExtBuilder::default().build().execute_with(|| {
            let bad_min_value = MinNominationPerCollator::get() - 1;
            let new_min_setting =
                AdminSettings::<BalanceOf<Test>>::MinTotalNominatorStake(bad_min_value);

            assert_noop!(
                ParachainStaking::set_admin_setting(Origin::root(), new_min_setting.clone()),
                Error::<Test>::AdminSettingsValueIsNotValid
            );
        });
    }
}

mod min_collator_stake_admin_setting {
    use super::*;

    #[test]
    fn can_be_updated() {
        ExtBuilder::default().build().execute_with(|| {
            let new_min_value = <MinCollatorStake<Test>>::get() - 1;
            let new_min_setting = AdminSettings::<BalanceOf<Test>>::MinCollatorStake(new_min_value);

            assert_ok!(ParachainStaking::set_admin_setting(
                Origin::root(),
                new_min_setting.clone()
            ));

            assert_eq!(<MinCollatorStake<Test>>::get(), new_min_value);
            assert_last_event!(MetaEvent::ParachainStaking(Event::AdminSettingsUpdated {
                value: new_min_setting
            }));
        });
    }

    #[test]
    fn can_be_set_to_0() {
        ExtBuilder::default().build().execute_with(|| {
            let new_min_value = 0;
            let new_min_setting = AdminSettings::<BalanceOf<Test>>::MinCollatorStake(new_min_value);

            assert_ok!(ParachainStaking::set_admin_setting(
                Origin::root(),
                new_min_setting.clone()
            ));

            assert_eq!(<MinCollatorStake<Test>>::get(), new_min_value);
            assert_last_event!(MetaEvent::ParachainStaking(Event::AdminSettingsUpdated {
                value: new_min_setting
            }));
        });
    }
}
