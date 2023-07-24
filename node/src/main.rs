//! AVN Parachain Node CLI

#![warn(missing_docs)]

mod chain_spec;
#[macro_use]
mod service;
mod avn_config;
mod cli;
mod command;
mod common;
mod rpc;

fn main() -> sc_cli::Result<()> {
	command::run()
}
