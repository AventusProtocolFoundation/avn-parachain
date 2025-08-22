// Copyright 2023 Aventus Network Services.
// This file is part of Aventus.

// AvN specific cli configuration
use clap::Parser;

#[derive(Debug, Parser)]
pub struct AvnCliConfiguration {
    pub avn_port: Option<String>,
    pub ethereum_node_url: Option<String>,
    pub finance_api_key: Option<String>,
}
