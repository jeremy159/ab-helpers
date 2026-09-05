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

For `actual.password` and `actual.sync_id`, prefer the `_FILE` env vars over
writing the real value into `config.toml` — see [Secrets](#secrets) below.

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

### Secrets

`actual.password` and `actual.sync_id` (and `database.password`, for the
server) are secrets. Avoid writing the real value into `config.toml` or a
committed `.env` file — instead, append `_FILE` to the variable name and
point it at a file holding just that value. The file's contents (trailing
newline trimmed) become the effective value. Setting both a variable and its
`_FILE` counterpart is an error, to avoid silently picking one. This is the
same convention Docker/Compose secrets and the official Postgres/MySQL
images use, so it works unchanged whether the file is a `chmod 600` file on
your machine or a Docker secret mount (see Docker below). `abh init` also
keeps `~/.config/ab-helpers/config.toml` itself owner-only (`0600`) in case
you do put a real value there directly.

**Local CLI setup:**

```bash
mkdir -p ~/.config/ab-helpers/secrets
printf '%s' 'your-password' > ~/.config/ab-helpers/secrets/actual_password.txt
printf '%s' 'your-sync-id'  > ~/.config/ab-helpers/secrets/actual_sync_id.txt
chmod 600 ~/.config/ab-helpers/secrets/*.txt
```

Then export the `_FILE` vars so every `abh` invocation picks them up — add
this to your shell profile (`~/.zshrc`, `~/.bashrc`, etc.):

```bash
export ABH_ACTUAL__PASSWORD_FILE=~/.config/ab-helpers/secrets/actual_password.txt
export ABH_ACTUAL__SYNC_ID_FILE=~/.config/ab-helpers/secrets/actual_sync_id.txt
```

### Actual bridge

Actual is driven from Rust through a small Node.js "bridge" script. When
`actual.bridge_script` (or `ABH_ACTUAL__BRIDGE_SCRIPT`) isn't set, it
self-installs on first run: a managed copy is written to
`$XDG_DATA_HOME/ab-helpers/bridge` (typically
`~/.local/share/ab-helpers/bridge`), and `npm ci` runs there automatically
the first time `node_modules` is missing or out of date. This requires
`node`/`npm` — already required for running the bridge at all — but no full
git checkout of this repo. By default `node`/`npm` are resolved from `PATH`;
if you set `node_bin` (or `ABH_ACTUAL__NODE_BIN`) to a specific `node`
binary not on `PATH`, `npm` is looked up next to it first, so pointing at a
non-`PATH` Node install (e.g. via nvm) works without needing `npm` on `PATH`
too. Subsequent runs skip the install and start immediately; a binary
upgrade that bumps the bridge's dependencies automatically triggers a fresh
`npm ci` the next time it runs. To force a full reinstall manually, delete
the managed directory (typically `rm -rf ~/.local/share/ab-helpers/bridge`)
and it will be recreated on the next run.

Set `bridge_script` explicitly to point at a real checkout instead (e.g. for
development, or to pin/customize the bridge):

```toml
[actual]
bridge_script = "/path/to/ab-helpers/crates/actual/bridge/index.js"
```

## Logging

Controlled via `RUST_LOG`. See [docs/logging.md](docs/logging.md) for level conventions and recommended values.

## Docker

Create the two secret files `compose.yaml` mounts into the container:

```bash
mkdir -p secrets
printf '%s' 'your-password' > secrets/actual_password.txt
printf '%s' 'your-sync-id'  > secrets/actual_sync_id.txt
chmod 600 secrets/actual_password.txt secrets/actual_sync_id.txt
```

`secrets/` is gitignored. Compose mounts each as a read-only file under
`/run/secrets/`, and the daemon reads them via `ABH_ACTUAL__PASSWORD_FILE` /
`ABH_ACTUAL__SYNC_ID_FILE`, already set in `compose.yaml` — the real values
never need to touch `.env` or any committed file.

Then create a `.env` file next to `compose.yaml` for the non-secret settings:

```env
ABH_ACTUAL__SERVER_URL=https://your-actual-server
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
