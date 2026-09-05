//! Rust client for Actual Budget.
//!
//! Actual ships a JS-only programmatic library (`@actual-app/api`); we drive
//! it from Rust through a small Node bridge living in `crates/actual/bridge`.
//! Each call spawns the bridge with a subcommand, sends JSON args, and reads
//! a single JSON line back.

mod bridge;
mod client;
mod error;
mod managed_bridge;
#[cfg(test)]
mod tests;
mod types;

pub use bridge::{BridgeConfig, BridgeInvoker};
pub use client::{ActualReadRequests, ActualWriteRequests, Client};
pub use error::{ActualResult, Error};
pub use managed_bridge::ensure_managed_bridge_script;
pub use types::{
    Account, AddTransactionResponse, BalanceResponse, ImportTransaction, LastTransaction,
    ListAccountsResponse, SaveTransaction,
};

#[cfg(feature = "testutils")]
pub use client::{MockActualReadRequestsImpl, MockActualWriteRequestsImpl};
