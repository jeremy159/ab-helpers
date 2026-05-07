# ab-helpers

Automation helpers for [Actual Budget](https://actualbudget.org/): applies loan interest, reconciles balances, and runs as a scheduled daemon.

## CLI subcommands

```
abh set-balance <account> <amount> [--dry-run]   # Reconcile an account to a target balance
abh apply-kia-interest       [--dry-run]          # Apply weekly Kia loan interest
abh apply-mortgage-interest  [--dry-run]          # Apply monthly mortgage interest
abh daemon                                        # Run the cron scheduler (production entry point)
```

## Installing the CLI locally

**Install / update** (builds release binary and puts `abh` in `~/.cargo/bin`):
```bash
cargo install --path crates/ab-helpers-cli
```

Re-running the same command updates to the latest version. `~/.cargo/bin` is in PATH by default for any Rust installation, so `abh` will be available immediately in a new shell.

## Configuration

Key environment variables (override any `base.toml` value with `ABH__SECTION__KEY`):

| Variable | Description |
|---|---|
| `ABH__ACTUAL__SERVER_URL` | Actual Budget server URL |
| `ABH__ACTUAL__PASSWORD` | Actual Budget password |
| `ABH__ACTUAL__SYNC_ID` | Budget file sync ID |
| `ABH__ACTUAL__DATA_DIR` | Path for Actual local data cache |
| `ABH__ACTUAL__KIA__ACCOUNT_ID` | Kia loan account ID in Actual |
| `ABH__ACTUAL__MORTGAGE__ACCOUNT_ID` | Mortgage account ID in Actual |
| `ABH__SCHEDULER__TIMEZONE` | Cron timezone (default: `America/New_York`) |
| `ABH__SCHEDULER__KIA_INTEREST_CRON` | Kia cron schedule (default: Thursdays 9 AM) |
| `ABH__SCHEDULER__MORTGAGE_INTEREST_CRON` | Mortgage cron schedule (default: 18th of month 9 AM) |

## Docker

**Build:**
```bash
docker build -t ab-helpers .
```

**Run daemon:**
```bash
docker run -d --restart unless-stopped --name ab-helpers \
  -v ab-helpers-data:/data \
  -e ABH__ACTUAL__SERVER_URL=https://your-actual-server \
  -e ABH__ACTUAL__PASSWORD=your-password \
  -e ABH__ACTUAL__SYNC_ID=your-sync-id \
  -e ABH__ACTUAL__KIA__ACCOUNT_ID=your-kia-account-id \
  -e ABH__ACTUAL__MORTGAGE__ACCOUNT_ID=your-mortgage-account-id \
  ab-helpers
```

**Update to a new image:**
```bash
docker build -t ab-helpers . && docker restart ab-helpers
```

**One-off command (e.g. dry-run):**
```bash
docker run --rm \
  -e ABH__ACTUAL__SERVER_URL=... \
  ab-helpers abh apply-kia-interest --dry-run
```
