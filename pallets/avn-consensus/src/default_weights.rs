use frame_support::weights::Weight;

pub trait WeightInfo {
    fn submit() -> Weight;
    fn clear_consensus() -> Weight;
}

impl WeightInfo for () {
    fn submit() -> Weight {
        Weight::zero()
    }
    fn clear_consensus() -> Weight {
        Weight::zero()
    }
}
