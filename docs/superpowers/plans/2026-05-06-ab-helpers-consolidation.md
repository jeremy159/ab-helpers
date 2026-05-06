# ab-helpers Consolidation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move the Rust workspace from `budgetize-server` into `ab-helpers`, rename all `budgetize-*` crates to `ab-helpers-*`, port the two JS interest scripts to Rust CLI subcommands, and replace the Node cron Docker container with `abh daemon`.

**Architecture:** The `crates/actual` bridge (Node subprocess + Rust client) stays unchanged except for 4 new subcommands. Business logic lives in `ab-helpers-server`. The `abh` binary exposes `set-balance`, `apply-kia-interest`, `apply-mortgage-interest`, and `daemon` subcommands. The daemon uses `tokio-cron-scheduler` and persists last-run times in `{data_dir}/daemon-state.json`.

**Tech Stack:** Rust 2021, tokio, clap 4, chrono + chrono-tz 0.10, tokio-cron-scheduler 0.13, cron 0.12, serde_json, mockall 0.13, Node.js (bridge only).

---

## File Map

### Copied from budgetize-server (unchanged)
- `Cargo.toml` ← workspace root
- `Cargo.lock`
- `.cargo/audit.toml`
- `compose.yaml`
- `scripts/init_db.sh`
- `crates/actual/` (all files)
- `crates/db-postgres/`
- `crates/db-redis/`

### Renamed (directory + package name)
- `crates/budgetize-cli/` → `crates/ab-helpers-cli/`
- `crates/budgetize-domain/` → `crates/ab-helpers-domain/`
- `crates/budgetize-server/` → `crates/ab-helpers-server/`

### Modified after rename
- `Cargo.toml` — workspace dep keys
- `crates/ab-helpers-cli/Cargo.toml` — package name, binary name (`abh`)
- `crates/ab-helpers-domain/Cargo.toml` — package name
- `crates/ab-helpers-server/Cargo.toml` — package name, dep refs
- `crates/actual/Cargo.toml` — dep rename
- All `.rs` files that import `budgetize_domain` or `budgetize_server`
- `crates/ab-helpers-server/src/config.rs` — env prefix, path

### New files
- `crates/actual/src/types.rs` — 4 new types
- `crates/actual/src/client.rs` — 4 new trait methods + mock stubs
- `crates/actual/bridge/index.js` — 4 new subcommands
- `crates/ab-helpers-domain/src/models/interest.rs` — `InterestOutcome`
- `crates/ab-helpers-server/src/services/actual/interest.rs` — `apply_bank_payment`, `mortgage_cutoff`, `InterestService`
- `crates/ab-helpers-server/configuration/base.toml` — new config sections
- `crates/ab-helpers-server/src/config.rs` — new config structs
- `crates/ab-helpers-cli/src/commands/mod.rs`
- `crates/ab-helpers-cli/src/commands/set_balance.rs` — extracted from main.rs
- `crates/ab-helpers-cli/src/commands/apply_kia_interest.rs`
- `crates/ab-helpers-cli/src/commands/apply_mortgage_interest.rs`
- `crates/ab-helpers-cli/src/commands/daemon.rs`
- `Dockerfile` — rewritten (multi-stage Rust + Node)

---

## Task 1: Copy Rust workspace into ab-helpers

**Files:** Copy from `../budgetize-server/` (relative to `ab-helpers/`)

- [ ] **Step 1: Copy workspace files**

Run from within `ab-helpers/`:
```bash
cp ../budgetize-server/Cargo.toml ./Cargo.toml
cp ../budgetize-server/Cargo.lock ./Cargo.lock
cp -r ../budgetize-server/.cargo ./.cargo
cp ../budgetize-server/compose.yaml ./compose.yaml
cp -r ../budgetize-server/scripts ./scripts
cp -r ../budgetize-server/crates ./crates
```

- [ ] **Step 2: Verify cargo check passes**

```bash
cargo check
```
Expected: compiles without errors (may warn about unused items).

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock .cargo compose.yaml scripts crates
git commit -m "chore: import rust workspace from budgetize-server"
```

---

## Task 2: Rename crates and update binary name

**Files:**
- Modify: `Cargo.toml`
- Rename + Modify: `crates/budgetize-cli/` → `crates/ab-helpers-cli/`
- Rename + Modify: `crates/budgetize-domain/` → `crates/ab-helpers-domain/`
- Rename + Modify: `crates/budgetize-server/` → `crates/ab-helpers-server/`
- Modify: `crates/actual/Cargo.toml`
- Modify: all `.rs` files importing `budgetize_*`

- [ ] **Step 1: Rename crate directories**

```bash
mv crates/budgetize-cli crates/ab-helpers-cli
mv crates/budgetize-domain crates/ab-helpers-domain
mv crates/budgetize-server crates/ab-helpers-server
```

- [ ] **Step 2: Rewrite root `Cargo.toml`**

Replace the entire file:
```toml
[workspace]
members = ["crates/*"]
resolver = "2"

[workspace.package]
rust-version = "1.94"
authors = ["Jeremy Dube <jeremy.dube.dev@gmail.com>"]
license = "MIT OR Apache-2.0"
edition = "2021"

[profile.dev]
debug = 0

[profile.release]
incremental = true
debug = 0

[workspace.dependencies]
# Local Crates
ab-helpers-domain = { path = "./crates/ab-helpers-domain" }
ab-helpers-server = { path = "./crates/ab-helpers-server" }
db-postgres = { path = "./crates/db-postgres" }
db-redis = { path = "./crates/db-redis" }
actual = { path = "./crates/actual" }

# Non-Local Crates
sqlx = { version = "0.8", features = ["runtime-tokio", "tls-rustls", "macros", "uuid", "chrono", "migrate", "json"] }
fred = { version = "9" }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
chrono = { version = "0.4", default-features = false, features = ["clock", "std", "serde"] }
chrono-tz = { version = "0.10", features = ["serde"] }
anyhow = { version = "1", features = ["backtrace"] }
thiserror = "2"
uuid = { version = "1", features = ["serde", "v4"] }
async-trait = "0.1"
tracing = "0.1"
futures = "0.3"
itertools = "0.13"
tokio-cron-scheduler = "0.13"
cron = "0.12"
```

- [ ] **Step 3: Rewrite `crates/ab-helpers-cli/Cargo.toml`**

```toml
[package]
name = "ab-helpers-cli"
version = "0.0.0"
authors.workspace = true
edition.workspace = true
license.workspace = true
rust-version.workspace = true

[[bin]]
name = "abh"
path = "src/main.rs"

[dependencies]
ab-helpers-domain.workspace = true
ab-helpers-server.workspace = true
actual.workspace = true

anyhow.workspace = true
tokio.workspace = true
clap = { version = "4", features = ["derive"] }
tracing.workspace = true
tracing-subscriber = { version = "0.3", features = ["env-filter", "registry"] }
chrono.workspace = true
chrono-tz.workspace = true
tokio-cron-scheduler.workspace = true
cron.workspace = true
serde.workspace = true
serde_json.workspace = true
```

- [ ] **Step 4: Rewrite `crates/ab-helpers-domain/Cargo.toml`**

```toml
[package]
name = "ab-helpers-domain"
version = "0.1.0"
rust-version.workspace = true
authors.workspace = true
license.workspace = true
edition.workspace = true

[features]
default = []
testutils = ["dep:mockall", "dep:fake", "dep:rand"]

[dependencies]
async-trait.workspace = true
thiserror.workspace = true
uuid.workspace = true
chrono.workspace = true
serde.workspace = true
serde_json.workspace = true
sqlx.workspace = true
fred.workspace = true

mockall = { version = "0.13", optional = true }
fake = { version = "3", optional = true, features = ["chrono", "derive", "uuid"] }
rand = { version = "0.8", optional = true }
```

- [ ] **Step 5: Rewrite `crates/ab-helpers-server/Cargo.toml`**

```toml
[package]
name = "ab-helpers-server"
version = "0.0.0"
authors.workspace = true
edition.workspace = true
license.workspace = true
rust-version.workspace = true

[lib]
path = "src/lib.rs"
doctest = false

[[bin]]
name = "ab-helpers-server"
path = "src/main.rs"

[dependencies]
ab-helpers-domain.workspace = true
db-postgres.workspace = true
db-redis.workspace = true
actual.workspace = true

axum = { version = "0.7", features = ["macros"] }
axum-extra = { version = "0.9", features = ["query", "form"] }
tower = "0.5"
tower-request-id = "0.3"
tower-http = { version = "0.6", features = ["trace", "cors", "fs"] }
http = "1"

tokio.workspace = true
futures.workspace = true
async-trait.workspace = true

serde.workspace = true
serde_json.workspace = true
chrono.workspace = true

config = { version = "0.14", features = ["toml"], default-features = false }

anyhow.workspace = true
thiserror.workspace = true

tracing.workspace = true
tracing-subscriber = { version = "0.3", features = ["env-filter", "registry"] }
tracing-log = "0.2"

uuid.workspace = true
secrecy = { version = "0.8", features = ["serde"] }
sqlx = { workspace = true, features = ["postgres"] }

[dev-dependencies]
ab-helpers-domain = { workspace = true, features = ["testutils"] }
reqwest = { version = "0.12", features = ["json", "cookies"] }
wiremock = "0.6"
once_cell = "1"
sqlx = { workspace = true, features = ["postgres"] }
```

- [ ] **Step 6: Update `crates/actual/Cargo.toml`** — rename `budgetize-domain` dep

In `crates/actual/Cargo.toml`, replace the line:
```toml
budgetize-domain.workspace = true
```
with:
```toml
ab-helpers-domain.workspace = true
```

- [ ] **Step 7: Fix all Rust import paths**

Run:
```bash
grep -rl "budgetize_domain\|budgetize_server\|budgetize_cli" crates/ --include="*.rs"
```

For each file found, replace:
- `budgetize_domain` → `ab_helpers_domain`
- `budgetize_server` → `ab_helpers_server`
- `budgetize_cli` → `ab_helpers_cli`

Also update the tracing filter string in `crates/ab-helpers-cli/src/main.rs`:
```rust
// change:
EnvFilter::new("budgetize_cli=info,actual=info")
// to:
EnvFilter::new("ab_helpers_cli=info,actual=info")
```

- [ ] **Step 8: Update `config.rs` env prefix and path**

In `crates/ab-helpers-server/src/config.rs`, make these two changes:

Change the `base_path` block:
```rust
let base_path = {
    let p = std::env::current_dir().expect("Failed to determine the current directory");
    if !p.ends_with("crates/ab-helpers-server") {
        p.join("crates/ab-helpers-server")
    } else {
        p
    }
};
```

Change the env var prefix:
```rust
.add_source(
    config::Environment::with_prefix("ABH")
        .prefix_separator("_")
        .separator("__"),
)
```

Change the `BUDGETIZE_ENVIRONMENT` reference:
```rust
let environment: Environment = std::env::var("ABH_ENVIRONMENT")
    .unwrap_or_else(|_| "local".into())
    .try_into()
    .expect("Failed to parse ABH_ENVIRONMENT.");
```

- [ ] **Step 9: Verify cargo check passes**

```bash
cargo check
```
Expected: compiles without errors.

- [ ] **Step 10: Commit**

```bash
git add -A
git commit -m "chore: rename budgetize-* crates to ab-helpers-*, binary to abh"
```

---

## Task 3: Add 4 new bridge subcommands + Rust types

**Files:**
- Modify: `crates/actual/bridge/index.js`
- Modify: `crates/actual/src/types.rs`
- Modify: `crates/actual/src/client.rs`
- Modify: `crates/actual/src/lib.rs`

- [ ] **Step 1: Add 4 subcommands to `crates/actual/bridge/index.js`**

Inside the `switch (subcommand)` block, add 4 new cases before `default`:

```js
      case "get-last-transaction":
        result = await getLastTransaction(args);
        break;
      case "get-balance-at":
        result = await getBalanceAt(args);
        break;
      case "ensure-payee":
        result = await ensurePayee(args);
        break;
      case "import-transaction":
        result = await importTransaction(args);
        break;
```

Then add the 4 implementation functions after `addTransaction`:

```js
async function getLastTransaction({ accountId }) {
  if (!accountId) {
    throwApi("missing-account-id", "accountId is required");
  }
  const data = await api.runQuery(
    api.q("transactions")
      .filter({ account: accountId })
      .select(["date", "amount"])
      .orderBy({ date: "desc" })
      .limit(1)
      .options({ splits: "grouped" })
  );
  if (!data.data.length) {
    throwApi("no-transactions", `no transactions found for account ${accountId}`);
  }
  const tx = data.data[0];
  return { date: tx.date, amount: Number(tx.amount) };
}

async function getBalanceAt({ accountId, date }) {
  if (!accountId) throwApi("missing-account-id", "accountId is required");
  if (!date) throwApi("missing-date", "date is required");
  const data = await api.runQuery(
    api.q("transactions")
      .filter({ account: accountId, date: { $lt: date } })
      .calculate({ $sum: "$amount" })
      .options({ splits: "grouped" })
  );
  return { balance: Number(data.data) };
}

async function ensurePayee({ name }) {
  if (!name) throwApi("missing-name", "name is required");
  const payees = await api.getPayees();
  let payee = payees.find(p => p.name === name);
  if (!payee) {
    const id = await api.createPayee({ name });
    return { id: String(id) };
  }
  return { id: String(payee.id) };
}

async function importTransaction({ accountId, date, payeeId, amount, notes, cleared }) {
  if (!accountId) throwApi("missing-account-id", "accountId is required");
  if (typeof amount !== "number" || !Number.isInteger(amount)) {
    throwApi("bad-amount", "amount must be an integer (cents)");
  }
  const tx = {
    account: accountId,
    date: date || new Date().toISOString().slice(0, 10),
    payee: payeeId || undefined,
    amount,
    notes: notes || undefined,
    cleared: cleared !== undefined ? cleared : false,
  };
  const result = await api.importTransactions(accountId, [tx]);
  const id = Array.isArray(result) ? result[0]
    : (result && result.added && result.added[0]);
  if (!id) {
    throwApi("transaction-not-created", "Actual returned no transaction id");
  }
  return { id: String(id) };
}
```

- [ ] **Step 2: Add new types to `crates/actual/src/types.rs`**

Append to the existing file (after `AddTransactionResponse`):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LastTransactionResponse {
    pub date: String,
    pub amount: i64,
}

#[derive(Debug, Clone)]
pub struct LastTransaction {
    pub date: chrono::NaiveDate,
    pub amount: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnsurePayeeResponse {
    pub id: String,
}

#[derive(Debug, Clone)]
pub struct ImportTransaction {
    pub account_id: String,
    pub date: chrono::NaiveDate,
    pub payee_id: String,
    pub amount: i64,
    pub notes: Option<String>,
    pub cleared: Option<bool>,
}

/// Wire format for import-transaction bridge call.
#[derive(Debug, Clone, Serialize)]
pub struct ImportTransactionRequest {
    #[serde(rename = "accountId")]
    pub account_id: String,
    pub date: String,
    #[serde(rename = "payeeId")]
    pub payee_id: String,
    pub amount: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cleared: Option<bool>,
}
```

Add `use chrono::NaiveDate;` at the top of `types.rs` (or use full path inline — using full path is fine too since `chrono` is already a dep).

- [ ] **Step 3: Add new trait methods and implementations to `crates/actual/src/client.rs`**

Extend `AccountRequests` trait:
```rust
#[async_trait]
pub trait AccountRequests: Send + Sync {
    async fn list_accounts(&self) -> ActualResult<Vec<Account>>;
    async fn get_account_balance(&self, account_id: &str) -> ActualResult<i64>;
    async fn get_last_transaction(&self, account_id: &str) -> ActualResult<LastTransaction>;
    async fn ensure_payee(&self, name: &str) -> ActualResult<String>;
}
```

Extend `TransactionRequests` trait:
```rust
#[async_trait]
pub trait TransactionRequests: Send + Sync {
    async fn add_transaction(&self, tx: SaveTransaction) -> ActualResult<AddTransactionResponse>;
    async fn get_balance_at(&self, account_id: &str, date: chrono::NaiveDate) -> ActualResult<i64>;
    async fn import_transaction(&self, tx: ImportTransaction) -> ActualResult<String>;
}
```

Add implementations to `impl AccountRequests for Client`:
```rust
    async fn get_last_transaction(&self, account_id: &str) -> ActualResult<LastTransaction> {
        let value = self
            .invoker
            .invoke("get-last-transaction", json!({ "accountId": account_id }))
            .await?;
        let resp: LastTransactionResponse = serde_json::from_value(value)?;
        let date = chrono::NaiveDate::parse_from_str(&resp.date, "%Y-%m-%d")
            .map_err(|e| Error::BridgeProtocol(format!("invalid date from bridge: {e}")))?;
        Ok(LastTransaction { date, amount: resp.amount })
    }

    async fn ensure_payee(&self, name: &str) -> ActualResult<String> {
        let value = self
            .invoker
            .invoke("ensure-payee", json!({ "name": name }))
            .await?;
        let resp: EnsurePayeeResponse = serde_json::from_value(value)?;
        Ok(resp.id)
    }
```

Add implementations to `impl TransactionRequests for Client`:
```rust
    async fn get_balance_at(&self, account_id: &str, date: chrono::NaiveDate) -> ActualResult<i64> {
        let value = self
            .invoker
            .invoke(
                "get-balance-at",
                json!({ "accountId": account_id, "date": date.to_string() }),
            )
            .await?;
        let resp: BalanceResponse = serde_json::from_value(value)?;
        Ok(resp.balance)
    }

    async fn import_transaction(&self, tx: ImportTransaction) -> ActualResult<String> {
        let req = ImportTransactionRequest {
            account_id: tx.account_id,
            date: tx.date.to_string(),
            payee_id: tx.payee_id,
            amount: tx.amount,
            notes: tx.notes,
            cleared: tx.cleared,
        };
        let value = self
            .invoker
            .invoke("import-transaction", serde_json::to_value(&req)?)
            .await?;
        let resp: AddTransactionResponse = serde_json::from_value(value)?;
        Ok(resp.id)
    }
```

Update the `mockall::mock!` blocks to include the new methods:

```rust
#[cfg(feature = "testutils")]
mockall::mock! {
    pub AccountRequestsImpl {}

    impl Clone for AccountRequestsImpl {
        fn clone(&self) -> Self;
    }

    #[async_trait]
    impl AccountRequests for AccountRequestsImpl {
        async fn list_accounts(&self) -> ActualResult<Vec<Account>>;
        async fn get_account_balance(&self, account_id: &str) -> ActualResult<i64>;
        async fn get_last_transaction(&self, account_id: &str) -> ActualResult<LastTransaction>;
        async fn ensure_payee(&self, name: &str) -> ActualResult<String>;
    }
}

#[cfg(feature = "testutils")]
mockall::mock! {
    pub TransactionRequestsImpl {}

    impl Clone for TransactionRequestsImpl {
        fn clone(&self) -> Self;
    }

    #[async_trait]
    impl TransactionRequests for TransactionRequestsImpl {
        async fn add_transaction(&self, tx: SaveTransaction) -> ActualResult<AddTransactionResponse>;
        async fn get_balance_at(&self, account_id: &str, date: chrono::NaiveDate) -> ActualResult<i64>;
        async fn import_transaction(&self, tx: ImportTransaction) -> ActualResult<String>;
    }
}
```

Update the `pub use` line in `crates/actual/src/lib.rs` to export the new types:
```rust
pub use types::{
    Account, AddTransactionResponse, BalanceResponse, ImportTransaction,
    LastTransaction, ListAccountsResponse, SaveTransaction,
};
```

- [ ] **Step 4: Verify cargo check passes**

```bash
cargo check
```

- [ ] **Step 5: Commit**

```bash
git add crates/actual/
git commit -m "feat(actual): add get-last-transaction, get-balance-at, ensure-payee, import-transaction"
```

---

## Task 4: `apply_bank_payment` pure function + tests

**Files:**
- Create: `crates/ab-helpers-server/src/services/actual/interest.rs`
- Modify: `crates/ab-helpers-server/src/services/actual/mod.rs`

- [ ] **Step 1: Write failing tests first**

Create `crates/ab-helpers-server/src/services/actual/interest.rs` with just the test module:

```rust
pub struct BankPaymentResult {
    pub interest: i64,
    pub principal: i64,
    pub new_balance: i64,
}

pub fn apply_bank_payment(
    previous_balance: i64,
    payment: i64,
    rate: f64,
    round: bool,
) -> BankPaymentResult {
    todo!()
}

/// Replicate JS mortgage cutoff: setDate(getMonth()-1); setDate(getDate()-1).
pub fn mortgage_cutoff(last_tx_date: chrono::NaiveDate) -> chrono::NaiveDate {
    todo!()
}

fn set_day_js(year: i32, month: u32, day: i64) -> chrono::NaiveDate {
    chrono::NaiveDate::from_ymd_opt(year, month, 1).unwrap()
        + chrono::Duration::days(day - 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    // apply_bank_payment tests

    #[test]
    fn interest_rounds_when_round_true() {
        // balance=-50000 (owe $500), rate=0.00133978648017598, round=true
        // abs_prev=50000, interest=round(50000 * 0.001339786)=round(66.989)=67
        let r = apply_bank_payment(-50000, 10000, 0.00133978648017598, true);
        assert_eq!(r.interest, -67);
    }

    #[test]
    fn interest_floors_when_round_false() {
        // Same as above but floor: floor(66.989)=66
        let r = apply_bank_payment(-50000, 10000, 0.00133978648017598, false);
        assert_eq!(r.interest, -66);
    }

    #[test]
    fn new_balance_negative_account() {
        // prev=-50000, interest_abs=67(rounded), payment=10000
        // new_balance = -50000 - 67 + 10000 = -40067
        let r = apply_bank_payment(-50000, 10000, 0.00133978648017598, true);
        assert_eq!(r.new_balance, -40067);
    }

    #[test]
    fn new_balance_positive_account() {
        // prev=50000 (asset), payment=0, interest=67
        // new_balance = 50000 + 67 - 0 = 50067
        let r = apply_bank_payment(50000, 0, 0.00133978648017598, true);
        assert_eq!(r.interest, 67);
        assert_eq!(r.new_balance, 50067);
    }

    #[test]
    fn zero_interest_when_zero_balance() {
        let r = apply_bank_payment(0, 0, 0.00133978648017598, true);
        assert_eq!(r.interest, 0);
        assert_eq!(r.new_balance, 0);
    }

    // mortgage_cutoff tests

    #[test]
    fn mortgage_cutoff_may() {
        // May 18: month0=4, step1=setDay(4-1)=May 3, step2=setDay(3-1)=May 2
        let d = NaiveDate::from_ymd_opt(2024, 5, 18).unwrap();
        assert_eq!(mortgage_cutoff(d), NaiveDate::from_ymd_opt(2024, 5, 2).unwrap());
    }

    #[test]
    fn mortgage_cutoff_february() {
        // Feb 18: month0=1, step1=setDay(0)=Jan 31, step2=setDay(30)=Jan 30
        let d = NaiveDate::from_ymd_opt(2024, 2, 18).unwrap();
        assert_eq!(mortgage_cutoff(d), NaiveDate::from_ymd_opt(2024, 1, 30).unwrap());
    }

    #[test]
    fn mortgage_cutoff_january() {
        // Jan 18: month0=0, step1=setDay(-1)=Dec 30 2023, step2=setDay(29)=Dec 29 2023
        let d = NaiveDate::from_ymd_opt(2024, 1, 18).unwrap();
        assert_eq!(mortgage_cutoff(d), NaiveDate::from_ymd_opt(2023, 12, 29).unwrap());
    }
}
```

Add to `crates/ab-helpers-server/src/services/actual/mod.rs`:
```rust
mod interest;
mod reconcile;

pub use interest::*;
pub use reconcile::*;
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
cargo test -p ab-helpers-server services::actual::interest
```
Expected: failures on `todo!()` panics.

- [ ] **Step 3: Implement `apply_bank_payment` and `mortgage_cutoff`**

Replace the `todo!()` bodies:

```rust
pub fn apply_bank_payment(
    previous_balance: i64,
    payment: i64,
    rate: f64,
    round: bool,
) -> BankPaymentResult {
    let abs_prev = previous_balance.unsigned_abs() as f64;
    let interest_abs = if round {
        (abs_prev * rate).round() as i64
    } else {
        (abs_prev * rate).floor() as i64
    };

    let new_balance = if previous_balance >= 0 {
        previous_balance + interest_abs - payment
    } else {
        previous_balance - interest_abs + payment
    };

    let interest_signed = if previous_balance < 0 { -interest_abs } else { interest_abs };
    let principal = previous_balance.unsigned_abs() as i64
        - new_balance.unsigned_abs() as i64;

    BankPaymentResult {
        interest: interest_signed,
        principal,
        new_balance,
    }
}

pub fn mortgage_cutoff(last_tx_date: chrono::NaiveDate) -> chrono::NaiveDate {
    let month0 = last_tx_date.month0() as i64; // 0-indexed (Jan=0)
    let year = last_tx_date.year();
    let month = last_tx_date.month();

    // JS: cutoff.setDate(cutoff.getMonth() - 1)
    let step1 = set_day_js(year, month, month0 - 1);

    // JS: cutoff.setDate(cutoff.getDate() - 1)
    set_day_js(step1.year(), step1.month(), step1.day() as i64 - 1)
}
```

- [ ] **Step 4: Run tests to confirm they pass**

```bash
cargo test -p ab-helpers-server services::actual::interest
```
Expected: all 8 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/ab-helpers-server/src/services/actual/
git commit -m "feat: add apply_bank_payment and mortgage_cutoff with tests"
```

---

## Task 5: `InterestOutcome` domain type + `InterestService`

**Files:**
- Create: `crates/ab-helpers-domain/src/models/interest.rs`
- Modify: `crates/ab-helpers-domain/src/models/mod.rs`
- Modify: `crates/ab-helpers-domain/src/lib.rs`
- Modify: `crates/ab-helpers-server/src/services/actual/interest.rs`

- [ ] **Step 1: Add `InterestOutcome` to domain**

Create `crates/ab-helpers-domain/src/models/interest.rs`:
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InterestOutcome {
    AccountClosed,
    NoInterest { balance: i64 },
    Applied {
        balance: i64,
        interest: i64,
        new_balance: i64,
        transaction_id: String,
    },
}
```

Add to `crates/ab-helpers-domain/src/models/mod.rs`:
```rust
mod actual;
mod interest;
mod money;

pub use actual::*;
pub use interest::*;
pub use money::*;
```

Ensure `crates/ab-helpers-domain/src/lib.rs` re-exports `models`:
```rust
pub mod db;
pub mod models;
pub use models::*;
```
(Check the existing lib.rs — add `pub use models::*;` if not already there.)

- [ ] **Step 2: Write failing `InterestService` tests**

Append to `crates/ab-helpers-server/src/services/actual/interest.rs`:

```rust
use std::sync::Arc;
use async_trait::async_trait;
use ab_helpers_domain::InterestOutcome;
use crate::error::{AppError, BudgetizeResult};

pub enum InterestPeriod {
    Weekly,
    Monthly,
}

pub struct InterestConfig {
    pub account_id: String,
    pub rate: f64,
    pub payee_name: String,
    pub round: bool,
    pub period: InterestPeriod,
}

pub struct InterestService<C> {
    client: Arc<C>,
    config: InterestConfig,
}

impl<C> InterestService<C> {
    pub fn new(client: Arc<C>, config: InterestConfig) -> Self {
        Self { client, config }
    }
}

pub trait ActualClient:
    actual::AccountRequests + actual::TransactionRequests + Send + Sync {}

impl<T> ActualClient for T where
    T: actual::AccountRequests + actual::TransactionRequests + Send + Sync {}

impl<C: ActualClient + 'static> InterestService<C> {
    pub async fn apply(&self) -> BudgetizeResult<InterestOutcome> {
        todo!()
    }
}

#[cfg(test)]
mod service_tests {
    use super::*;
    use std::sync::Arc;
    use async_trait::async_trait;
    use chrono::NaiveDate;
    use actual::{
        Account, ActualResult, AddTransactionResponse, ImportTransaction,
        LastTransaction, SaveTransaction,
    };

    struct FakeClient {
        accounts: Vec<Account>,
        last_tx: LastTransaction,
        balance: i64,
        payee_id: String,
        imported_tx: std::sync::Mutex<Option<ImportTransaction>>,
    }

    #[async_trait]
    impl actual::AccountRequests for FakeClient {
        async fn list_accounts(&self) -> ActualResult<Vec<Account>> {
            Ok(self.accounts.clone())
        }
        async fn get_account_balance(&self, _id: &str) -> ActualResult<i64> {
            Ok(self.balance)
        }
        async fn get_last_transaction(&self, _id: &str) -> ActualResult<LastTransaction> {
            Ok(self.last_tx.clone())
        }
        async fn ensure_payee(&self, _name: &str) -> ActualResult<String> {
            Ok(self.payee_id.clone())
        }
    }

    #[async_trait]
    impl actual::TransactionRequests for FakeClient {
        async fn add_transaction(&self, _tx: SaveTransaction) -> ActualResult<AddTransactionResponse> {
            Ok(AddTransactionResponse { id: "ignored".into() })
        }
        async fn get_balance_at(&self, _id: &str, _date: NaiveDate) -> ActualResult<i64> {
            Ok(self.balance)
        }
        async fn import_transaction(&self, tx: ImportTransaction) -> ActualResult<String> {
            *self.imported_tx.lock().unwrap() = Some(tx);
            Ok("tx-interest-1".into())
        }
    }

    fn make_account(id: &str, closed: bool) -> Account {
        Account { id: id.into(), name: "Test Loan".into(), offbudget: false, closed }
    }

    fn make_client(closed: bool) -> Arc<FakeClient> {
        Arc::new(FakeClient {
            accounts: vec![make_account("acc-1", closed)],
            last_tx: LastTransaction {
                date: NaiveDate::from_ymd_opt(2024, 5, 18).unwrap(),
                amount: 10000,
            },
            balance: -50000,
            payee_id: "payee-1".into(),
            imported_tx: Default::default(),
        })
    }

    fn kia_config() -> InterestConfig {
        InterestConfig {
            account_id: "acc-1".into(),
            rate: 0.00133978648017598,
            payee_name: "Loan Interest".into(),
            round: false,
            period: InterestPeriod::Weekly,
        }
    }

    #[tokio::test]
    async fn returns_account_closed_when_closed() {
        let svc = InterestService::new(make_client(true), kia_config());
        let outcome = svc.apply().await.unwrap();
        assert!(matches!(outcome, InterestOutcome::AccountClosed));
    }

    #[tokio::test]
    async fn applies_interest_and_imports_transaction() {
        let client = make_client(false);
        let svc = InterestService::new(client.clone(), kia_config());
        let outcome = svc.apply().await.unwrap();

        match outcome {
            InterestOutcome::Applied { interest, transaction_id, .. } => {
                assert_eq!(interest, -66); // floor(50000 * 0.00133978...) = 66, signed negative
                assert_eq!(transaction_id, "tx-interest-1");
            }
            other => panic!("unexpected: {other:?}"),
        }

        let tx = client.imported_tx.lock().unwrap().clone().expect("tx imported");
        assert_eq!(tx.account_id, "acc-1");
        assert_eq!(tx.payee_id, "payee-1");
        assert_eq!(tx.cleared, Some(true));
        assert!(tx.notes.as_deref().unwrap_or("").contains("semaine"));
    }

    #[tokio::test]
    async fn returns_no_interest_when_zero() {
        let client = Arc::new(FakeClient {
            accounts: vec![make_account("acc-1", false)],
            last_tx: LastTransaction {
                date: NaiveDate::from_ymd_opt(2024, 5, 18).unwrap(),
                amount: 0,
            },
            balance: 0, // zero balance → zero interest
            payee_id: "p".into(),
            imported_tx: Default::default(),
        });
        let svc = InterestService::new(client, kia_config());
        let outcome = svc.apply().await.unwrap();
        assert!(matches!(outcome, InterestOutcome::NoInterest { .. }));
    }
}
```

- [ ] **Step 3: Run tests to confirm they fail**

```bash
cargo test -p ab-helpers-server service_tests
```
Expected: failures on `todo!()`.

- [ ] **Step 4: Implement `InterestService::apply`**

Replace `todo!()` in the `apply` method:

```rust
    pub async fn apply(&self) -> BudgetizeResult<InterestOutcome> {
        // 1. Find account by ID
        let accounts = self.client.list_accounts().await.map_err(AppError::from_actual)?;
        let account = accounts
            .iter()
            .find(|a| a.id == self.config.account_id)
            .ok_or_else(|| AppError::ActualAccountNotFound(self.config.account_id.clone()))?;

        // 2. Closed guard
        if account.closed {
            tracing::warn!(
                account_id = %account.id,
                "account is closed, skipping interest run"
            );
            return Ok(InterestOutcome::AccountClosed);
        }

        // 3. Last transaction
        let last_tx = self.client
            .get_last_transaction(&account.id)
            .await
            .map_err(AppError::from_actual)?;

        // 4. Cutoff date
        let cutoff = match self.config.period {
            InterestPeriod::Weekly => last_tx.date - chrono::Duration::days(1),
            InterestPeriod::Monthly => mortgage_cutoff(last_tx.date),
        };

        // 5. Balance at cutoff
        let balance = self.client
            .get_balance_at(&account.id, cutoff)
            .await
            .map_err(AppError::from_actual)?;

        // 6. Compute interest
        let result = apply_bank_payment(balance, last_tx.amount, self.config.rate, self.config.round);

        if result.interest == 0 {
            return Ok(InterestOutcome::NoInterest { balance });
        }

        // 7. Notes string with formatted rate
        let rate_pct = format!("{:.2}%", self.config.rate * 100.0);
        let period_label = match self.config.period {
            InterestPeriod::Weekly => "semaine",
            InterestPeriod::Monthly => "mois",
        };
        let notes = format!("Intérêt pour 1 {period_label} à {rate_pct}");

        // 8. Ensure payee
        let payee_id = self.client
            .ensure_payee(&self.config.payee_name)
            .await
            .map_err(AppError::from_actual)?;

        // 9. Import transaction
        let import_tx = actual::ImportTransaction {
            account_id: account.id.clone(),
            date: last_tx.date,
            payee_id,
            amount: result.interest,
            notes: Some(notes),
            cleared: Some(true),
        };
        let transaction_id = self.client
            .import_transaction(import_tx)
            .await
            .map_err(AppError::from_actual)?;

        Ok(InterestOutcome::Applied {
            balance,
            interest: result.interest,
            new_balance: result.new_balance,
            transaction_id,
        })
    }
```

- [ ] **Step 5: Run tests to confirm they pass**

```bash
cargo test -p ab-helpers-server
```
Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/ab-helpers-domain/ crates/ab-helpers-server/src/services/actual/interest.rs
git commit -m "feat: add InterestService with closed-guard, cutoff math, and import-transaction"
```

---

## Task 6: Config structs + new dependencies

**Files:**
- Modify: `crates/ab-helpers-server/src/config.rs`
- Modify: `crates/ab-helpers-server/configuration/base.toml`
- Modify: `crates/ab-helpers-server/src/services/actual/interest.rs`

- [ ] **Step 1: Add config structs to `config.rs`**

Add the following structs to `crates/ab-helpers-server/src/config.rs` (after `ActualSettings`):

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct KiaSettings {
    pub account_id: String,
    pub weekly_rate: f64,
    pub payee_name: String,
    pub round: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MortgageSettings {
    pub account_id: String,
    pub monthly_rate: f64,
    pub payee_name: String,
    pub round: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SchedulerSettings {
    pub kia_interest_cron: String,
    pub mortgage_interest_cron: String,
    pub timezone: String,
}
```

Extend `ActualSettings` with the two loan sub-configs:
```rust
#[derive(Debug, Clone, Deserialize)]
pub struct ActualSettings {
    pub server_url: String,
    pub password: Secret<String>,
    pub sync_id: String,
    #[serde(default)]
    pub e2e_password: Option<Secret<String>>,
    #[serde(default)]
    pub data_dir: String,
    #[serde(default = "default_node_bin")]
    pub node_bin: String,
    #[serde(default)]
    pub bridge_script: String,
    pub kia: KiaSettings,
    pub mortgage: MortgageSettings,
}
```

Extend `Settings` with `scheduler`:
```rust
#[derive(Debug, Clone, Deserialize)]
pub struct Settings {
    pub application: ApplicationSettings,
    pub database: DatabaseSettings,
    pub redis: RedisSettings,
    pub actual: ActualSettings,
    pub scheduler: SchedulerSettings,
}
```

Add `interest_config()` helpers on the settings structs. Add at the bottom of `config.rs`:

```rust
use crate::services::actual::{InterestConfig, InterestPeriod};

impl KiaSettings {
    pub fn interest_config(&self) -> InterestConfig {
        InterestConfig {
            account_id: self.account_id.clone(),
            rate: self.weekly_rate,
            payee_name: self.payee_name.clone(),
            round: self.round,
            period: InterestPeriod::Weekly,
        }
    }
}

impl MortgageSettings {
    pub fn interest_config(&self) -> InterestConfig {
        InterestConfig {
            account_id: self.account_id.clone(),
            rate: self.monthly_rate,
            payee_name: self.payee_name.clone(),
            round: self.round,
            period: InterestPeriod::Monthly,
        }
    }
}
```

- [ ] **Step 2: Add new sections to `base.toml`**

Append to `crates/ab-helpers-server/configuration/base.toml`:

```toml
[actual.kia]
account_id   = ""
weekly_rate  = 0.00133978648017598
payee_name   = "Loan Interest"
round        = false

[actual.mortgage]
account_id   = ""
monthly_rate = 0.003543453216552734375
payee_name   = "Loan Interest"
round        = true

[scheduler]
kia_interest_cron      = "0 9 * * 4"
mortgage_interest_cron = "0 9 18 * *"
timezone               = "America/New_York"
```

Also add the account IDs to `crates/ab-helpers-server/configuration/local.toml`:
```toml
[actual.kia]
account_id = "a1d08c47-9e63-4f46-bd36-d4380098844c"

[actual.mortgage]
account_id = "eda51ae0-7510-4382-b6d7-2748ccb7f219"
```

- [ ] **Step 3: Verify cargo check passes**

```bash
cargo check
```
Expected: compiles without errors.

- [ ] **Step 4: Commit**

```bash
git add crates/ab-helpers-server/src/config.rs \
        crates/ab-helpers-server/configuration/base.toml \
        crates/ab-helpers-server/configuration/local.toml
git commit -m "feat: add KiaSettings, MortgageSettings, SchedulerSettings to config"
```

---

## Task 7: `apply-kia-interest` and `apply-mortgage-interest` CLI subcommands

**Files:**
- Create: `crates/ab-helpers-cli/src/commands/mod.rs`
- Create: `crates/ab-helpers-cli/src/commands/set_balance.rs`
- Create: `crates/ab-helpers-cli/src/commands/apply_kia_interest.rs`
- Create: `crates/ab-helpers-cli/src/commands/apply_mortgage_interest.rs`
- Modify: `crates/ab-helpers-cli/src/main.rs`

- [ ] **Step 1: Create `commands/mod.rs`**

```rust
pub mod apply_kia_interest;
pub mod apply_mortgage_interest;
pub mod set_balance;
```

- [ ] **Step 2: Extract `set_balance.rs` from existing `main.rs`**

Create `crates/ab-helpers-cli/src/commands/set_balance.rs` by moving the existing
`set_balance`, `run_dry_run`, and `print_outcome` functions out of `main.rs` into this file.
Add the necessary imports at the top:

```rust
use std::process::ExitCode;
use std::sync::Arc;

use ab_helpers_domain::{Money, ReconcileOutcome};
use ab_helpers_server::config::Settings;
use ab_helpers_server::error::AppError;
use ab_helpers_server::services::actual::{ReconcileOptions, ReconcileService, ReconcileServiceExt};
use clap::Args;

#[derive(Args, Debug)]
pub struct SetBalanceArgs {
    pub account: String,
    pub amount: Money,
    #[arg(long)]
    pub date: Option<String>,
    #[arg(long, default_value = "Balance Adjustment")]
    pub payee_name: String,
    #[arg(long)]
    pub notes: Option<String>,
    #[arg(long)]
    pub dry_run: bool,
}

pub async fn run(settings: Settings, args: SetBalanceArgs) -> anyhow::Result<ExitCode> {
    // (paste the body of the old `set_balance` function here)
}

// (paste run_dry_run and print_outcome here too)
```

- [ ] **Step 3: Create `apply_kia_interest.rs`**

```rust
use std::process::ExitCode;
use std::sync::Arc;

use ab_helpers_domain::InterestOutcome;
use ab_helpers_server::config::Settings;
use ab_helpers_server::services::actual::{InterestService};

pub async fn run(settings: Settings) -> anyhow::Result<ExitCode> {
    let client = Arc::new(settings.actual.client());
    let config = settings.actual.kia.interest_config();
    let service = InterestService::new(client, config);

    match service.apply().await {
        Ok(InterestOutcome::AccountClosed) => {
            println!("Account is closed — skipping.");
            Ok(ExitCode::SUCCESS)
        }
        Ok(InterestOutcome::NoInterest { balance }) => {
            println!("No interest to apply. Balance: {} cents", balance);
            Ok(ExitCode::SUCCESS)
        }
        Ok(InterestOutcome::Applied { balance, interest, new_balance, transaction_id }) => {
            println!("Balance:      {} cents", balance);
            println!("Interest:     {} cents", interest);
            println!("New balance:  {} cents", new_balance);
            println!("Transaction:  {}", transaction_id);
            Ok(ExitCode::SUCCESS)
        }
        Err(err) => {
            eprintln!("error: {err:?}");
            Ok(ExitCode::from(3))
        }
    }
}
```

- [ ] **Step 4: Create `apply_mortgage_interest.rs`**

Identical to `apply_kia_interest.rs` except uses `settings.actual.mortgage.interest_config()`:

```rust
use std::process::ExitCode;
use std::sync::Arc;

use ab_helpers_domain::InterestOutcome;
use ab_helpers_server::config::Settings;
use ab_helpers_server::services::actual::InterestService;

pub async fn run(settings: Settings) -> anyhow::Result<ExitCode> {
    let client = Arc::new(settings.actual.client());
    let config = settings.actual.mortgage.interest_config();
    let service = InterestService::new(client, config);

    match service.apply().await {
        Ok(InterestOutcome::AccountClosed) => {
            println!("Account is closed — skipping.");
            Ok(ExitCode::SUCCESS)
        }
        Ok(InterestOutcome::NoInterest { balance }) => {
            println!("No interest to apply. Balance: {} cents", balance);
            Ok(ExitCode::SUCCESS)
        }
        Ok(InterestOutcome::Applied { balance, interest, new_balance, transaction_id }) => {
            println!("Balance:      {} cents", balance);
            println!("Interest:     {} cents", interest);
            println!("New balance:  {} cents", new_balance);
            println!("Transaction:  {}", transaction_id);
            Ok(ExitCode::SUCCESS)
        }
        Err(err) => {
            eprintln!("error: {err:?}");
            Ok(ExitCode::from(3))
        }
    }
}
```

- [ ] **Step 5: Rewrite `main.rs`**

```rust
use std::process::ExitCode;

use ab_helpers_server::config::Settings;
use anyhow::Context;
use clap::{Parser, Subcommand};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

mod commands;

/// abh — Actual Budget Helpers CLI.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Reconcile an account balance to a target value.
    SetBalance(commands::set_balance::SetBalanceArgs),
    /// Apply weekly Kia loan interest.
    ApplyKiaInterest,
    /// Apply monthly mortgage interest.
    ApplyMortgageInterest,
    /// Run the daemon scheduler (production entry point).
    Daemon,
}

#[tokio::main]
async fn main() -> ExitCode {
    init_tracing();
    match run().await {
        Ok(code) => code,
        Err(err) => {
            eprintln!("error: {err:?}");
            ExitCode::from(3)
        }
    }
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("ab_helpers_cli=info,actual=info"));
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();
}

async fn run() -> anyhow::Result<ExitCode> {
    let args = Cli::parse();
    let settings = Settings::build().context("failed to load configuration")?;

    match args.command {
        Commands::SetBalance(a) => commands::set_balance::run(settings, a).await,
        Commands::ApplyKiaInterest => commands::apply_kia_interest::run(settings).await,
        Commands::ApplyMortgageInterest => commands::apply_mortgage_interest::run(settings).await,
        Commands::Daemon => commands::daemon::run(settings).await,
    }
}
```

Add `pub mod daemon;` to `commands/mod.rs` (will implement in Task 8).
Create a stub `commands/daemon.rs` so it compiles now:

```rust
use std::process::ExitCode;
use ab_helpers_server::config::Settings;

pub async fn run(_settings: Settings) -> anyhow::Result<ExitCode> {
    todo!("daemon not yet implemented")
}
```

- [ ] **Step 6: Verify cargo check passes**

```bash
cargo check -p ab-helpers-cli
```
Expected: compiles.

- [ ] **Step 7: Commit**

```bash
git add crates/ab-helpers-cli/
git commit -m "feat: add apply-kia-interest and apply-mortgage-interest CLI subcommands"
```

---

## Task 8: `daemon` subcommand

**Files:**
- Modify: `crates/ab-helpers-cli/src/commands/daemon.rs`

- [ ] **Step 1: Implement `daemon.rs`**

Replace the stub with the full implementation:

```rust
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use ab_helpers_server::config::Settings;
use ab_helpers_server::services::actual::InterestService;
use anyhow::Context;
use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use cron::Schedule;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use tokio::sync::Mutex;
use tokio_cron_scheduler::{Job, JobScheduler};

#[derive(Debug, Serialize, Deserialize, Default)]
struct DaemonState {
    kia_interest_last_run: Option<DateTime<Utc>>,
    mortgage_interest_last_run: Option<DateTime<Utc>>,
}

fn state_path(data_dir: &str) -> PathBuf {
    PathBuf::from(data_dir).join("daemon-state.json")
}

fn load_state(data_dir: &str) -> DaemonState {
    let path = state_path(data_dir);
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_state(data_dir: &str, state: &DaemonState) {
    let path = state_path(data_dir);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(state) {
        let _ = std::fs::write(&path, json);
    }
}

/// Returns true if the next scheduled tick after `last_run` is already in the past.
fn tick_was_missed(cron_5field: &str, last_run: DateTime<Utc>) -> bool {
    let expr = format!("0 {cron_5field}"); // prepend seconds field
    let Ok(schedule) = Schedule::from_str(&expr) else { return false };
    schedule.after(&last_run).next().map_or(false, |next| next < Utc::now())
}

pub async fn run(settings: Settings) -> anyhow::Result<ExitCode> {
    let tz: Tz = settings.scheduler.timezone.parse()
        .context("invalid timezone in scheduler config")?;

    let data_dir = settings.actual.data_dir.clone();
    let state = Arc::new(Mutex::new(load_state(&data_dir)));

    // --- Missed-tick catch-up on startup ---
    {
        let s = state.lock().await;
        let kia_cron = settings.scheduler.kia_interest_cron.clone();
        let mort_cron = settings.scheduler.mortgage_interest_cron.clone();

        let kia_missed = s.kia_interest_last_run
            .map_or(true, |t| tick_was_missed(&kia_cron, t));
        let mort_missed = s.mortgage_interest_last_run
            .map_or(true, |t| tick_was_missed(&mort_cron, t));

        drop(s); // release lock before async calls

        if kia_missed {
            tracing::info!("kia interest tick was missed — running now");
            run_kia(&settings, &state, &data_dir).await;
        }
        if mort_missed {
            tracing::info!("mortgage interest tick was missed — running now");
            run_mortgage(&settings, &state, &data_dir).await;
        }
    }

    // --- Cron scheduler ---
    let scheduler = JobScheduler::new().await?;

    {
        let settings_kia = settings.clone();
        let state_kia = Arc::clone(&state);
        let data_dir_kia = data_dir.clone();
        let kia_expr = format!("0 {}", settings.scheduler.kia_interest_cron);

        let kia_job = Job::new_async_tz(&kia_expr, tz, move |_uuid, _lock| {
            let s = settings_kia.clone();
            let st = Arc::clone(&state_kia);
            let dd = data_dir_kia.clone();
            Box::pin(async move {
                tracing::info!("scheduler: running kia interest");
                run_kia(&s, &st, &dd).await;
            })
        })?;
        scheduler.add(kia_job).await?;
    }

    {
        let settings_mort = settings.clone();
        let state_mort = Arc::clone(&state);
        let data_dir_mort = data_dir.clone();
        let mort_expr = format!("0 {}", settings.scheduler.mortgage_interest_cron);

        let mort_job = Job::new_async_tz(&mort_expr, tz, move |_uuid, _lock| {
            let s = settings_mort.clone();
            let st = Arc::clone(&state_mort);
            let dd = data_dir_mort.clone();
            Box::pin(async move {
                tracing::info!("scheduler: running mortgage interest");
                run_mortgage(&s, &st, &dd).await;
            })
        })?;
        scheduler.add(mort_job).await?;
    }

    tracing::info!("daemon started");
    scheduler.start().await?;

    // Block forever — the scheduler runs background tasks.
    tokio::signal::ctrl_c().await.ok();
    tracing::info!("shutting down");
    scheduler.shutdown().await?;

    Ok(ExitCode::SUCCESS)
}

async fn run_kia(settings: &Settings, state: &Arc<Mutex<DaemonState>>, data_dir: &str) {
    let client = Arc::new(settings.actual.client());
    let config = settings.actual.kia.interest_config();
    let service = InterestService::new(client, config);

    match service.apply().await {
        Ok(outcome) => {
            tracing::info!(?outcome, "kia interest applied");
            let mut s = state.lock().await;
            s.kia_interest_last_run = Some(Utc::now());
            save_state(data_dir, &s);
        }
        Err(err) => {
            tracing::error!(?err, "kia interest failed — will retry next scheduled tick");
        }
    }
}

async fn run_mortgage(settings: &Settings, state: &Arc<Mutex<DaemonState>>, data_dir: &str) {
    let client = Arc::new(settings.actual.client());
    let config = settings.actual.mortgage.interest_config();
    let service = InterestService::new(client, config);

    match service.apply().await {
        Ok(outcome) => {
            tracing::info!(?outcome, "mortgage interest applied");
            let mut s = state.lock().await;
            s.mortgage_interest_last_run = Some(Utc::now());
            save_state(data_dir, &s);
        }
        Err(err) => {
            tracing::error!(?err, "mortgage interest failed — will retry next scheduled tick");
        }
    }
}
```

Note: `InterestOutcome` must implement `Debug` (already derived in Task 5).

- [ ] **Step 2: Add `tokio::signal` feature to tokio workspace dep**

In root `Cargo.toml`, update:
```toml
tokio = { version = "1", features = ["full"] }
```
`"full"` already includes `signal`, so no change needed.

- [ ] **Step 3: Verify cargo check passes**

```bash
cargo check -p ab-helpers-cli
```
Expected: compiles.

- [ ] **Step 4: Commit**

```bash
git add crates/ab-helpers-cli/src/commands/daemon.rs
git commit -m "feat: add daemon subcommand with cron scheduler and missed-tick detection"
```

---

## Task 9: Rewrite Dockerfile

**Files:**
- Modify: `Dockerfile`

- [ ] **Step 1: Rewrite `Dockerfile`**

Replace the entire file:

```dockerfile
# Stage 1: compile Rust
FROM rust:1.87-alpine AS rust-builder
RUN apk add --no-cache musl-dev pkgconfig openssl-dev
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/
RUN cargo build --release -p ab-helpers-cli

# Stage 2: install Node bridge deps
FROM node:20-alpine AS node-setup
WORKDIR /bridge
COPY crates/actual/bridge/package*.json ./
RUN npm ci --omit=dev

# Stage 3: runtime
FROM debian:bookworm-slim AS runner
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates nodejs \
    && rm -rf /var/lib/apt/lists/*

COPY --from=rust-builder /build/target/release/abh /usr/local/bin/abh
COPY --from=node-setup /bridge/node_modules /app/bridge/node_modules
COPY crates/actual/bridge/index.js /app/bridge/index.js

ENV NODE_ENV=production
ENV ACTUAL_DATA_DIR=/data
ENV ABH_ENVIRONMENT=production

VOLUME ["/data"]

CMD ["abh", "daemon"]
```

Update `crates/ab-helpers-server/src/config.rs` so the `bridge_script` default resolves to `/app/bridge/index.js` in production. The existing logic already handles a blank `bridge_script` by looking for `crates/actual/bridge/index.js` relative to the workspace root. In the Docker container, set:

```
ABH__ACTUAL__BRIDGE_SCRIPT=/app/bridge/index.js
```

Add that env var to the Dockerfile:
```dockerfile
ENV ABH__ACTUAL__BRIDGE_SCRIPT=/app/bridge/index.js
```

- [ ] **Step 2: Verify Docker build**

```bash
docker build -t abh:local .
```
Expected: build completes successfully.

- [ ] **Step 3: Smoke-test the container starts**

```bash
docker run --rm abh:local abh --version
```
Expected: prints version string without error.

- [ ] **Step 4: Commit**

```bash
git add Dockerfile
git commit -m "feat: multi-stage Dockerfile — Rust binary + Node bridge, CMD abh daemon"
```

---

## Task 10: Cleanup old JS files + final verification

**Files:**
- Delete: `api.js`, `utils.js`, `apply-kia-interest.js`, `apply-mortgage-interest.js`, `index-cron.js`, `package.json`, `node_modules/`

- [ ] **Step 1: Delete old JS root files**

```bash
rm api.js utils.js apply-kia-interest.js apply-mortgage-interest.js index-cron.js package.json
rm -rf node_modules/
```

- [ ] **Step 2: Run full test suite**

```bash
cargo test
```
Expected: all tests pass.

- [ ] **Step 3: Run cargo check in release mode**

```bash
cargo build --release -p ab-helpers-cli
```
Expected: compiles without errors.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "chore: remove old JS scripts and node_modules — fully ported to Rust"
```

---

## Self-Review Checklist

- [x] Spec Section 1 (rename): Tasks 1–2
- [x] Spec Section 2 (bridge subcommands + Rust traits): Task 3
- [x] Spec Section 3 (InterestService, apply_bank_payment, mortgage_cutoff, daemon): Tasks 4–5, 8
- [x] Spec Section 4 (config): Task 6
- [x] Spec Section 5 (testing): Tasks 4–5 (TDD throughout)
- [x] Spec Section 6 (Dockerfile): Task 9
- [x] Implementation order from spec: Tasks 1→10 match spec order exactly
- [x] Closed account guard: Task 5 step 4, `if account.closed { return AccountClosed }`
- [x] `round` config per account: Task 6 step 1, `KiaSettings.round`, `MortgageSettings.round`
- [x] Mortgage cutoff replicates JS: Task 4 step 3 (`set_day_js` helper)
- [x] Missed-tick detection: Task 8 step 1 (`tick_was_missed`)
- [x] Log-and-continue on daemon error: Task 8 step 1 (`run_kia`/`run_mortgage`)
- [x] `import-transaction` uses `api.importTransactions` (dedup): Task 3 step 1
- [x] Notes string with formatted rate: Task 5 step 4
- [x] JS files stay until Task 9 Docker verified, deleted in Task 10
