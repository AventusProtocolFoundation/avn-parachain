# Parachain staking pallet

A DPoS Pallet for Parachain Staking

This project is originally a fork of the
[Moonbeam parachain-staking plallet](https://github.com/PureStake/moonbeam/tree/v0.26.1/pallets/parachain-staking) forked at [tag](https://github.com/PureStake/moonbeam/tree/v0.26.1).

## Parachain Staking
Minimal staking pallet that implements collator selection by total backed stake.
The main difference between this pallet and `frame/pallet-staking` is that this pallet
uses direct nomination. Nominators choose exactly who they nominate and with what stake.
This is different from `frame/pallet-staking` where nominators approval vote and run Phragmen.

#### Rules
There is a new era every `<Era<T>>::get().length` blocks.

At the start of every era,
* issuance is calculated for collators (and their nominators) for block authoring
`T::RewardPaymentDelay::get()` eras ago
* a new set of collators is chosen from the candidates

Immediately following a era change, payments are made once-per-block until all payments have
been made. In each such block, one collator is chosen for a rewards payment and is paid along
with each of its top `T::MaxTopNominationsPerCandidate` nominators.

To join the set of candidates, call `join_candidates` with `bond >= MinCollatorStake`.
To leave the set of candidates, call `schedule_leave_candidates`. If the call succeeds,
the collator is removed from the pool of candidates so they cannot be selected for future
collator sets, but they are not unbonded until their exit request is executed. Any signed
account may trigger the exit `<Delay<T>>::get()` eras after the era in which the
original request was made.

To join the set of nominators, call `nominate` and pass in an account that is
already a collator candidate and `bond >= MinTotalNominatorStake`. Each nominator can nominate
up to `T::MaxNominationsPerNominator` collator candidates by calling `nominate`. Nominations
increase the `total stake` the collator has in the system.

The `T::MinNominationPerCollator` constant defines the minimum amount of stake that a nominator
must contribute to a specific collator in order for the nomination to be valid.

To revoke a nomination, call `revoke_nomination` with the collator candidate's account.
To leave the set of nominators and revoke all nominations, call `leave_nominators`.

### Reward Distribution
Rewards are distributed to collators and their nominators based on their contributions to the
network.

#### How Rewards Are Calculated

1. **Block Authoring Points**:
   - Candidates earn points for authoring blocks via the `note_author` callback.

2. **Reward Distribution**:
   - Each candidate's reward share is proportional to the points they earned relative to the
     total points in the era. This fraction is multiplied by the allocated
     `total_staking_reward` for the era.

3. **Reward Source**:
   - The `total_staking_reward` is deducted from the `RewardPotId` account, which is set by the
     pallet configuration. The pallet does not have visibility into how this account is funded.
     In the Aventus Network (AvN), this account is credited with all transaction fees as
     configured in the runtime. If the account does not have enough funds, the
     total_staking_reward is set to 0.

4. The Candidate's reward shared
is further divided proportionally based on their **total stake**, which includes:
   - **Candidate Stake**: The amount staked by the collator themselves.
   - **Nominators' Stake**: The total amount staked by nominators backing the collator.
5. **Candidate Rewards**: Candidate' rewards are proportional to their individual stake relative
to the total stake backing the collator.
6. **Nominator Rewards**: Nominators' rewards are proportional to their individual stake
relative to the total stake backing the collator. This is repeated for each nomination an
account has on different candidates.

#### Reward Formulas
total_reward_for_candidate = (collator_points / total_points) * total_staking_reward
candidate_reward = (candidate_stake / candidate_total_stake) * total_reward_for_candidate
nomination_reward = (nomination_stake / candidate_total_stake) * total_reward_for_candidate

- `collator_points`: Total points the collator earned in the era.
- `total_points`: Total points earned by all collators in the era.
- `total_staking_reward`: Total reward available in the reward pot for the era.
- `candidate_stake`: The amount staked by the collator themselves.
- `candidate_total_stake`: Total stake backing the collator (self-bond + nominations stake).
- `nomination_stake`: Stake contributed by the nominator.

#### Delayed Payouts
- Rewards are paid after a delay of `T::RewardPaymentDelay` eras.
- During each block, one collator and their nominators are paid until all rewards for the era
  are distributed.

### Growth Mechanism

The growth mechanism is a process that increases the total token supply
across two tiers. A growth period spans multiple eras, defined by `T::ErasPerGrowthPeriod`.
It rewards collators based on their performance over multiple eras.

#### Conditions
- **Activation**: The growth mechanism is enabled only if the `T::GrowthEnabled` constant is set
  to `true`.
- **Deactivation**: Growth is skipped if:
   - The number of accumulations is zero.
   - The total stake or total rewards for the growth period is zero.

#### How It Works
1. **Triggering Growth**:
   - If `T::GrowthEnabled` is set to `true`, growth events are triggered every
     `T::ErasPerGrowthPeriod` eras. These values are configured as part of the pallet setup.
   - At the beginning of a new era, the system checks whether the conditions for initiating a
     new growth event are met. If the conditions are satisfied, the `trigger_growth_on_t1`
     function is invoked. This function sends a request to the bridge contract to mint new
     tokens. The pallet is not opinionated on how the growth is implemented or how the
     information provided is used by the formula in tier 1. It simply provides the following
     data:
     - `total_stake`: The total average stake in the system at the start of the growth period.
     - `total_rewards`: The total rewards distributed to collators and nominators during the
     growth period.
   - The bridge contract processes the request and emits an `AvtGrowthLiftedData` event once the
     tokens are successfully minted and transferred to the parachain.

2. **Processing Growth**:
   - The parachain listens for the `AvtGrowthLiftedData` event.
   - Once the event is processed, the `OnGrowthLiftedHandler::on_growth_lifted` callback is
     triggered to distribute the newly minted tokens to collators.
   - Token manager that typically handles the token minting and lifting process can be
     configured to reserve a portion for the treasury.

3. **Reward Distribution**:
   - Growth payments are distributed in addition to regular staking rewards.
   - The performance of collators (measured by their points) determines the amount of growth
     rewards they receive.