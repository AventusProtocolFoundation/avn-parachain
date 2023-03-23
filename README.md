# AvN Node

![image](./res/aventus.jpg)

Aventus Network belongs to the new generation of composable blockchain networks built for scalability and interoperability 🚀. It is capable of high transaction throughput and low and predictable transaction costs. The network currently operates as a parachain to Polkadot.

## Open Sourcing

The purpose of this repo is to gradually open-source the Aventus parachain code and also give the community the opportunity to contribute to the Aventus codebase either by providing enhancements to the already available code or contributing new pallets.

We are very keen on community engagement and contributions.

## Contributions Guidelines

We welcome contributions but before you devote quite a bit of time to contributing, you should make sure you're certain your contribution has not already been addressed.

Read our contribution guide [HERE](./CONTRIBUTING.adoc).

Note: This repository is managed frequently so you do not need to email/contact us to notify us of your submission.

## Building and testing the pallets

To build and test a pallet, navigate to the directory of the pallet in your project by running the following command:
```sh
cd pallets/<pallet_name>/
```
Once you're in the pallet directory, use cargo to build and test the pallet by running the following commands:
```sh
# Build the pallet
cargo build

# Run the unit and integration tests
cargo test

# Test the benchmark tests
cargo test --features runtime-benchmarks
```
