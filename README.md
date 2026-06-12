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

Configuration is loaded by layering, with later sources overriding earlier ones:
`base.toml` (defaults) → an overlay file → `ABH_`-prefixed environment variables.
The source is resolved in this order (first match wins):

1. `ABH_CONFIG_FILE` — a single explicit config file.
2. `ABH_CONFIG_DIR` — `base.toml` + `<ABH_ENVIRONMENT>.toml` in that directory.
3. `configuration/` next to the binary (how the Docker image is set up).
4. `~/.config/ab-helpers/{base.toml,config.toml}` — the installed CLI (see below).
5. The project's `configuration/` directory (development).

### CLI config (`~/.config/ab-helpers`)

For the installed CLI, run `abh init` once to seed your config:

```bash
abh init
```

This copies `base.toml` (the defaults floor) to `~/.config/ab-helpers/base.toml`
and writes a starter `~/.config/ab-helpers/config.toml` for your overrides. Edit
`config.toml` to set your Actual credentials and account IDs — you only need the
fields that differ from `base.toml`. Re-run `abh init` anytime to refresh
`base.toml` with project updates; your `config.toml` is left untouched (use
`--force` to overwrite it).

### Environment variables

Override any value with `ABH_SECTION__KEY` (single underscore after the `ABH`
prefix, double underscore between nested keys):

| Variable | Description |
|---|---|
| `ABH_ACTUAL__SERVER_URL` | Actual Budget server URL |
| `ABH_ACTUAL__PASSWORD` | Actual Budget password |
| `ABH_ACTUAL__SYNC_ID` | Budget file sync ID |
| `ABH_ACTUAL__CACHE_DIR` | Path for the Actual local data cache + daemon state |
| `ABH_ACTUAL__KIA__ACCOUNT_ID` | Kia loan account ID in Actual |
| `ABH_ACTUAL__MORTGAGE__ACCOUNT_ID` | Mortgage account ID in Actual |
| `ABH_SCHEDULER__TIMEZONE` | Cron timezone (default: `America/New_York`) |
| `ABH_SCHEDULER__KIA_INTEREST_CRON` | Kia cron schedule (default: Thursdays 9 AM) |
| `ABH_SCHEDULER__MORTGAGE_INTEREST_CRON` | Mortgage cron schedule (default: 18th of month 9 AM) |

## Logging

Controlled via `RUST_LOG`. See [docs/logging.md](docs/logging.md) for level conventions and recommended values.

## Docker

Create a `.env` file next to `compose.yaml` with your credentials:

```env
ABH_ACTUAL__SERVER_URL=https://your-actual-server
ABH_ACTUAL__PASSWORD=your-password
ABH_ACTUAL__SYNC_ID=your-sync-id
ABH_ACTUAL__KIA__ACCOUNT_ID=your-kia-account-id
ABH_ACTUAL__MORTGAGE__ACCOUNT_ID=your-mortgage-account-id
```

**Start the daemon:**
```bash
docker compose up -d --build
```

**Update to a new image:**
```bash
docker compose up -d --build
```

**Follow logs:**
```bash
docker compose logs -f daemon
```

**One-off command (e.g. dry-run):**
```bash
docker compose run --rm daemon abh apply-kia-interest --dry-run
```
