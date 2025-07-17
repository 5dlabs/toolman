#![allow(clippy::uninlined_format_args)]

pub mod common;
pub mod real_servers;
pub mod simple_forwarding_test;
pub mod toolman_server_tests;
pub mod http_transport_tests;

// Re-export common utilities for tests
pub use common::*;

use std::sync::Once;

static INIT: Once = Once::new();

pub fn setup_integration_tests() {
    INIT.call_once(|| {
        // Set up logging for integration tests
        env_logger::Builder::from_default_env()
            .filter_level(log::LevelFilter::Info)
            .init();

        println!("Integration test environment initialized");
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_integration_framework() {
        setup_integration_tests();
        // Integration framework setup successful
    }
}
