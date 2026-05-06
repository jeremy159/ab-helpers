# Design — ab-helpers consolidation

Date: 2026-05-06
Status: Approved, pending implementation.

## Goal

Consolidate `budgetize-server` (Rust workspace) into `ab-helpers` (the canonical
repo for Actual Budget helper tooling), rename all `budgetize-*` crates to
`ab-helpers-*`, port the two JS interest scripts to Rust, and replace the
Node-based Docker cron container with a compiled Rust binary (`abh daemon`).

## High-level shape

```
ab-helpers/                          ← canonical repo (was: ab-helpers JS-only)
  Cargo.toml                         ← workspace root (moved from budgetize-server)
  Cargo.lock
  Dockerfile                         ← multi-stage: Rust build + Node bridge install
  compose.yaml
  crates/
    actual/                          ← unchanged (bridge JS + Rust client)
      bridge/index.js                ← extended with 4 new subcommands
      src/
    ab-helpers-cli/                  ← was: budgetize-cli; binary name: abh
      src/
        commands/
          set_balance.rs
          apply_kia_interest.rs      ← new
          apply_mortgage_interest.rs ← new
          daemon.rs                  ← new
    ab-helpers-domain/               ← was: budgetize-domain
      src/
        models/
          actual.rs                  ← gains InterestOutcome
    ab-helpers-server/               ← was: budgetize-server (also serves as core lib)
      src/
        services/
          actual/
            reconcile.rs             ← existing
            interest.rs              ← new (shared business logic)
    db-postgres/                     ← unchanged
    db-redis/                        ← unchanged
  docs/
  scripts/
```

Old JS files removed at the end of implementation (kept until Docker build is
verified): `api.js`, `utils.js`, `apply-kia-interest.js`,
`apply-mortgage-interest.js`, `index-cron.js`, root `package.json`.

## Section 1: Repo structure & renaming

All `budgetize-*` crate names and Cargo package names change to `ab-helpers-*`.
The workspace `Cargo.toml` `members = [...]` paths update to match the new
directory names. Internal crate dependency references update accordingly.

The CLI binary name changes from `budgetize-cli` to `abh`:

```toml
# crates/ab-helpers-cli/Cargo.toml
[[bin]]
name = "abh"
path = "src/main.rs"
```

Usage:
```
abh set-balance <account> <amount>
abh apply-kia-interest
abh apply-mortgage-interest
abh daemon
```

## Section 2: New bridge subcommands

Four subcommands added to `crates/actual/bridge/index.js`:

| Subcommand             | Args (JSON)                                             | Stdout (JSON)                         |
|------------------------|---------------------------------------------------------|---------------------------------------|
| `get-last-transaction` | `{accountId}`                                           | `{date: "YYYY-MM-DD", amount: <int>}` |
| `get-balance-at`       | `{accountId, date: "YYYY-MM-DD"}`                       | `{balance: <int cents>}`              |
| `ensure-payee`         | `{name}`                                               | `{id: <string>}`                      |
| `import-transaction`   | `{accountId, date, payeeId, amount, notes?, cleared?}`  | `{id: <string>}`                      |

Implementation notes:
- `get-last-transaction` queries `ORDER BY date DESC` to get the most recent transaction for the account.
- `get-balance-at` uses `api.runQuery` with `date: { $lt: date }` (same approach as `utils.js`), passing a `YYYY-MM-DD` string matching Actual's date comparison format.
- `import-transaction` calls `api.importTransactions` (not `addTransactions`) to get Actual's built-in import deduplication — if the daemon fires twice for the same payment (restart, cron drift), Actual will not create a duplicate transaction.
- `cleared` is optional in the bridge JSON and defaults to `false`.

Corresponding Rust trait additions in `crates/actual/src/client.rs`:

```rust
// AccountRequests
async fn get_last_transaction(&self, account_id: &str) -> ActualResult<LastTransaction>;
async fn ensure_payee(&self, name: &str) -> ActualResult<String>; // returns payee id

// TransactionRequests
async fn get_balance_at(&self, account_id: &str, date: NaiveDate) -> ActualResult<i64>;
async fn import_transaction(&self, tx: ImportTransaction) -> ActualResult<String>;
```

New types in `crates/actual/src/types.rs`:

```rust
pub struct LastTransaction { pub date: NaiveDate, pub amount: i64 }

pub struct ImportTransaction {
    pub account_id: String,
    pub date: NaiveDate,
    pub payee_id: String,
    pub amount: i64,
    pub notes: Option<String>,
    pub cleared: Option<bool>,
}
```

## Section 3: New CLI subcommands + daemon

### `apply-kia-interest` and `apply-mortgage-interest`

Both follow the same flow, implemented in
`ab-helpers-server/src/services/actual/interest.rs` as a generic
`InterestService` parameterised by an `InterestConfig` (containing account ID,
rate, payee name, rounding mode, and period — weekly or monthly).

Steps per run:

1. Read `InterestConfig` from settings (account ID, rate, payee name, `round` bool).
2. Fetch the account via `list-accounts` and verify it is not closed. If closed, log a warning and return early.
3. `get-last-transaction` → `{date, amount}` (most recent payment).
4. Compute cutoff date:
   - Kia (weekly): `last_tx_date − 1 day`
   - Mortgage (monthly): set day-of-month to `(0-indexed month − 1)`, then subtract 1 day — replicates exact JS behavior (`cutoff.setDate(cutoff.getMonth() - 1); cutoff.setDate(cutoff.getDate() - 1)`)
5. `get-balance-at` cutoff → balance (integer cents).
6. Compute interest via pure Rust `apply_bank_payment(balance, payment, rate, round)` where `round` is read from config.
7. Format the configured rate as a percentage string (e.g. `"0.13%"`) for the notes field.
8. If interest ≠ 0: `ensure-payee(payee_name)`, then `import-transaction` with `cleared: true` and notes `"Intérêt pour 1 semaine à {rate}%"` / `"Intérêt pour 1 mois à {rate}%"`.
9. Print summary to stdout: account name, balance, payment date, interest amount, new balance.

The `apply_bank_payment(balance, payment, rate, round)` function is pure (no I/O):

```rust
pub fn apply_bank_payment(
    previous_balance: i64,
    payment: i64,
    rate: f64,
    round: bool,  // true = round to nearest cent, false = floor
) -> BankPaymentResult {
    // ...
}

pub struct BankPaymentResult {
    pub interest: i64,
    pub principal: i64,
    pub new_balance: i64,
}
```

### `daemon`

Uses `tokio-cron-scheduler` (6-field cron with seconds — config values are
automatically prepended with `0 ` seconds field when parsed).

Schedules come from config:

```toml
[scheduler]
kia_interest_cron      = "0 9 * * 4"     # every Thursday 9 AM
mortgage_interest_cron = "0 9 18 * *"    # 18th of month 9 AM
timezone               = "America/New_York"
```

**Missed-tick detection:** On startup, the daemon reads a persistent state file
(`~/.local/share/abh/last_run.json` or `$ABH_STATE_DIR/last_run.json`) that
records the last successful run time per job. If the expected tick was missed
(i.e. now > next scheduled time after last run), the daemon runs the job
immediately before entering the scheduler loop.

**Error handling:** If a tick fails (Actual offline, bridge error, etc.), the
daemon logs the error with full detail and continues. The last-run timestamp is
only updated on success, so a failed tick will be retried on the next scheduled
firing (not immediately).

**Overlapping ticks:** Each job holds a mutex; a new tick is skipped (with a
log warning) if the previous run is still in progress.

**Lifecycle per tick:** The bridge subprocess is spawned fresh per API call
(same as the existing one-shot bridge model). No long-lived Node process.

**Note on "no subprocess":** The daemon calls `InterestService` directly in
Rust — it does not shell out to `abh apply-kia-interest` as a subprocess. The
bridge Node process is still spawned per API call within the service.

Docker `CMD` changes from `["node", "index-cron.js"]` to `["abh", "daemon"]`.

## Section 4: Config additions

New sections in `crates/ab-helpers-server/configuration/base.toml`:

```toml
[actual.kia]
account_id   = ""                      # filled in local.toml
weekly_rate  = 0.00133978648017598
payee_name   = "Loan Interest"
round        = false                   # floor to nearest cent (matches JS)

[actual.mortgage]
account_id   = ""                      # filled in local.toml
monthly_rate = 0.003543453216552734375
payee_name   = "Loan Interest"
round        = true                    # round to nearest cent (matches JS)

[scheduler]
kia_interest_cron      = "0 9 * * 4"
mortgage_interest_cron = "0 9 18 * *"
timezone               = "America/New_York"
```

Account IDs are left blank in `base.toml` and populated in `local.toml`
(already gitignored) alongside the existing Actual connection secrets.

**Migration note:** The old JS env var `INTEREST_PAYEE_NAME` is replaced by
`actual.kia.payee_name` and `actual.mortgage.payee_name` in config. There is no
env-var override path in the Rust port; update `local.toml` accordingly.

## Section 5: Testing

- **`apply_bank_payment` math** — pure unit tests covering both `round = true`
  and `round = false`, positive and negative balances. No bridge or mock needed.
- **`InterestService`** — unit-tested with `mockall` mocks of `AccountRequests`
  + `TransactionRequests` (same `testutils` feature gate as `ReconcileService`).
  Tests cover: normal run, account closed (early return), interest = 0 (no
  transaction created), and bridge error (propagated correctly).
- **New bridge subcommands** — tested with fake Node scripts in
  `crates/actual/tests/`, same pattern as existing bridge tests.
- **`daemon`** — not unit-tested; it is wiring only (scheduler fires → service
  called). Missed-tick logic is tested independently as a pure function.

## Section 6: Dockerfile

Multi-stage build replacing the current Node-only Dockerfile in `ab-helpers`:

```dockerfile
# Stage 1: compile Rust
FROM rust:alpine AS rust-builder
WORKDIR /build
COPY . .
RUN cargo build --release -p ab-helpers-cli

# Stage 2: install Node bridge deps
FROM node:alpine AS node-setup
WORKDIR /bridge
COPY crates/actual/bridge/package*.json ./
RUN npm ci --omit=dev

# Stage 3: runtime
FROM debian:bookworm-slim AS runner
COPY --from=rust-builder /build/target/release/abh /usr/local/bin/abh
COPY --from=node-setup /bridge/node_modules /app/bridge/node_modules
COPY crates/actual/bridge/index.js /app/bridge/index.js

ENV NODE_ENV=production
ENV ACTUAL_DATA_DIR=/data
VOLUME ["/data"]

CMD ["abh", "daemon"]
```

Env vars: `ACTUAL_SERVER_URL`, `ACTUAL_PASSWORD`, `ACTUAL_SYNC_ID`,
`ACTUAL_E2E_PASSWORD` (optional), `ACTUAL_DATA_DIR` (defaults to `/data`).
The `/data` volume persists the budget cache and daemon state across restarts.

## Implementation order

1. Rename crates (`budgetize-*` → `ab-helpers-*`), rename binary to `abh`, update workspace `Cargo.toml` members; `cargo check` passes.
2. Move workspace into `ab-helpers` repo (Rust files only); root JS files remain untouched during porting.
3. Add 4 new bridge subcommands to `index.js` + corresponding Rust trait methods and types.
4. Implement `apply_bank_payment` pure function + unit tests.
5. Implement `InterestService` in `ab-helpers-server/src/services/actual/interest.rs` + mockall unit tests.
6. Add config structs (`KiaSettings`, `MortgageSettings`, `SchedulerSettings`) and wire into `Settings`.
7. Wire `apply-kia-interest` and `apply-mortgage-interest` CLI subcommands.
8. Implement `daemon` subcommand (missed-tick detection, cron loop, error handling, overlap guard).
9. Rewrite Dockerfile; verify `docker build` succeeds and `abh daemon` starts cleanly.
10. Delete remaining JS root files (`api.js`, `utils.js`, `apply-kia-interest.js`, `apply-mortgage-interest.js`, `index-cron.js`, root `package.json`); `cargo test` passes.

## Out of scope (v1)

- HTTP routes for the interest commands (service is structured for it).
- Multi-loan generalisation beyond Kia and mortgage.
- Live Actual server integration tests.
- Automatic retry on failed ticks (failed ticks are logged; next scheduled firing retries naturally).
