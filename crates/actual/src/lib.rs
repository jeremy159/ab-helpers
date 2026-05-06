//! Rust client for Actual Budget.
//!
//! Actual ships a JS-only programmatic library (`@actual-app/api`); we drive
//! it from Rust through a small Node bridge living in `crates/actual/bridge`.
//! Each call spawns the bridge with a subcommand, sends JSON args, and reads
//! a single JSON line back.

mod bridge;
mod client;
mod error;
mod types;

pub use bridge::{BridgeConfig, BridgeInvoker};
pub use client::{AccountRequests, Client, TransactionRequests};
pub use error::{ActualResult, Error};
pub use types::{Account, AddTransactionResponse, BalanceResponse, ListAccountsResponse, SaveTransaction};

#[cfg(feature = "testutils")]
pub use client::{MockAccountRequestsImpl, MockTransactionRequestsImpl};
