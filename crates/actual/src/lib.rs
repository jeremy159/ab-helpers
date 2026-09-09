//! Rust client for Actual Budget.
//!
//! Actual ships a JS-only programmatic library (`@actual-app/api`); we drive
//! it from Rust through a small Node bridge living in `crates/actual/bridge`.
//!
//! The preferred path is [`BridgeSession`] (`BridgeConfig::open_session`):
//! one `node index.js serve` process, and one open Actual budget, reused for
//! every call that makes up a single logical operation (e.g. one CLI
//! command), talking newline-delimited JSON over its stdin/stdout. This
//! avoids re-downloading/re-syncing the whole local budget replica per call,
//! and gives a consistent snapshot across a command's several reads.
//!
//! `BridgeConfig` itself also implements [`BridgeInvoker`] directly, as a
//! single-shot fallback: each call spawns the bridge fresh with one
//! subcommand and reads one JSON line back. Kept for callers that only ever
//! make one call.

mod bridge;
mod client;
mod error;
mod lock;
mod managed_bridge;
mod session;
#[cfg(test)]
mod tests;
mod types;

pub use bridge::{BridgeConfig, BridgeInvoker, BridgeRequest, BridgeTimeouts};
pub use client::{ActualReadRequests, ActualWriteRequests, Client};
pub use error::{ActualResult, Error};
pub use managed_bridge::ensure_managed_bridge_script;
pub use session::BridgeSession;
pub use types::{
    Account, AddTransactionResponse, BalanceResponse, ImportTransaction, LastTransaction,
    ListAccountsResponse, SaveTransaction,
};

#[cfg(feature = "testutils")]
pub use client::{MockActualReadRequestsImpl, MockActualWriteRequestsImpl};
