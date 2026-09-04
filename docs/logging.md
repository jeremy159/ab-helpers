# Logging

Logging is controlled via the `RUST_LOG` environment variable (parsed by [`tracing-subscriber`](https://docs.rs/tracing-subscriber)).

**Default filter** (applied when `RUST_LOG` is not set) depends on the command:

- `abh daemon` (what Docker runs, unattended, no TTY — tracing is its only output): `abh=info,actual=info`
- Every other one-shot command (`init`, `set-balance`, `apply-*-interest`): `abh=warn,actual=warn` — these already report their outcome via plain stdout, so tracing only surfaces anomalies/errors by default.

`RUST_LOG` overrides this for any command, e.g. `RUST_LOG=abh=info abh set-balance ...`.

## Level conventions

| Level | When it fires |
|-------|--------------|
| `error` | Operation failed — nothing was written to Actual. Requires attention. |
| `warn` | Handled anomaly — e.g. account not found, ambiguous name, corrupt state file, silent I/O failure. Execution continued but something was skipped or fell back to a default. |
| `info` | Significant lifecycle milestone an operator cares about: command started, outcome achieved, daemon started/stopped, missed-tick decisions. Visible at the default filter level. |
| `debug` | Developer/troubleshooting detail: resolved config values, intermediate steps, function-level entry points. |
| `trace` | Fine-grained internal state: computed values mid-flow, file I/O steps. |

## Recommended `RUST_LOG` values

```bash
# Default — shows outcomes and lifecycle events only
RUST_LOG=abh=info

# Troubleshooting a specific command or the daemon
RUST_LOG=abh=debug

# Deep-dive into state transitions (very verbose)
RUST_LOG=abh=trace
```

## Docker / daemon

`compose.yaml` sets `RUST_LOG=abh=debug` for the daemon so missed-tick
decisions and job scheduling steps appear in `docker compose logs`. Override in your
`.env` file if you want a different level:

```env
RUST_LOG=abh=info   # quieter
RUST_LOG=abh=trace  # very verbose
```
