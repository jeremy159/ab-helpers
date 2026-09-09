#!/usr/bin/env node
//
// Bridge between the Rust `actual` crate and `@actual-app/api`.
//
// Two protocols:
//
//   Session mode (preferred - one Node process per CLI command, reused
//   across all of that command's calls instead of spawning fresh per call):
//     node index.js serve
//   Newline-delimited JSON on stdin/stdout, one request in flight at a time:
//     -> {"id":1,"operation":"open","args":{}}
//     <- {"id":1,"ok":{"accounts":[...]}}
//     -> {"id":2,"operation":"list-accounts","args":{}}
//     <- {"id":2,"ok":{"accounts":[...]}}
//     -> {"id":3,"operation":"close","args":{}}
//     <- {"id":3,"ok":{}}
//   Errors: {"id":N,"error":{"code":"...","message":"...","fatal":bool}}.
//   `fatal:true` means the session can no longer be trusted and the process
//   is about to exit; any other error leaves the session usable for further
//   operations. Every mutating operation (`add-transaction`,
//   `import-transaction`, `ensure-payee`) syncs to the server before
//   replying, so a write is never left stranded only in the local replica.
//
//   Single-shot mode (legacy - kept for manual debugging, e.g.
//   `npm run list-accounts`):
//     node index.js <subcommand> --json '<args-json>'
//   One JSON line on stdout: the subcommand's result, or
//   `{"error":{"code":"...","message":"..."}}` with exit code 1.
//
// All connection config is provided via env vars to keep secrets off argv:
//   ACTUAL_SERVER_URL, ACTUAL_PASSWORD, ACTUAL_SYNC_ID,
//   ACTUAL_E2E_PASSWORD (optional), ACTUAL_DATA_DIR.

"use strict";

const fs = require("fs");
const readline = require("readline");
const crypto = require("crypto");

// Reserve real stdout (fd 1) for the protocol. Redirect all to stderr *before* requiring the library, and
// only ever write protocol frames through `writeRaw`/`writeFrame` below,
// which write directly to fd 1.
process.stdout.write = function (chunk, encoding, callback) {
  return process.stderr.write(chunk, encoding, callback);
};
for (const method of ["log", "info", "debug", "trace", "group", "groupEnd"]) {
  console[method] = (...args) => console.error(...args);
}

/** Writes `text` directly to fd 1, retrying on partial writes / EAGAIN. */
function writeRaw(text) {
  const buf = Buffer.from(text, "utf8");
  let off = 0;
  while (off < buf.length) {
    try {
      off += fs.writeSync(1, buf, off, buf.length - off);
    } catch (err) {
      if (err && err.code === "EAGAIN") continue;
      throw err;
    }
  }
}

function writeFrame(obj) {
  writeRaw(JSON.stringify(obj) + "\n");
}

let api;
try {
  api = require("@actual-app/api");
} catch (err) {
  writeFrame({
    error: {
      code: "bridge-load-failed",
      message: `failed to require @actual-app/api: ${err.message}. Did you run \`npm install\` in crates/actual/bridge?`,
      fatal: true,
    },
  });
  process.exit(1);
}

process.on("uncaughtException", (err) => {
  writeFrame({
    id: null,
    error: { code: "unhandled", message: describeError(err), fatal: true },
  });
  process.exit(1);
});

process.on("unhandledRejection", (err) => {
  writeFrame({
    id: null,
    error: { code: "unhandled", message: describeError(err), fatal: true },
  });
  process.exit(1);
});

if (process.argv[2] === "serve") {
  serve();
} else {
  main().catch((err) => {
    if (err && err.__bridge) {
      emitError(err.__bridge.code, err.__bridge.message);
    } else {
      emitError("unhandled", describeError(err));
    }
    process.exit(1);
  });
}

// ---------------------------------------------------------------------------
// Session mode
// ---------------------------------------------------------------------------

/** Operations that mutate the budget and must be pushed to the server before
 * replying, so a crash right after a successful write never leaves it
 * stranded only in the local replica. */
const WRITE_OPERATIONS = new Set([
  "add-transaction",
  "import-transaction",
  "ensure-payee",
]);

const OPERATIONS = {
  "list-accounts": async () => accountsToResult(await api.getAccounts()),
  "get-balance": getBalance,
  "add-transaction": addTransaction,
  "get-last-transaction": getLastTransaction,
  "get-balance-at": getBalanceAt,
  "ensure-payee": ensurePayee,
  "import-transaction": importTransaction,
};

async function serve() {
  let opened = false;
  const rl = readline.createInterface({
    input: process.stdin,
    crlfDelay: Infinity,
  });

  try {
    for await (const line of rl) {
      if (!line.trim()) continue;

      let req;
      try {
        req = JSON.parse(line);
      } catch (err) {
        writeFrame({
          id: null,
          error: {
            code: "bad-request-json",
            message: `request line was not valid JSON: ${err.message}`,
            fatal: false,
          },
        });
        continue;
      }

      const { id, operation, args } = req && typeof req === "object" ? req : {};
      const outcome = await handleOperation(operation, args || {}, opened);
      opened = outcome.opened;
      writeFrame({ id: id === undefined ? null : id, ...outcome.frame });

      if (
        operation === "close" ||
        (outcome.frame.error && outcome.frame.error.fatal)
      ) {
        break;
      }
    }
  } finally {
    // Covers: a clean `close` (shutdown already ran, this is a harmless
    // no-op), a fatal error mid-session, and the parent going away (stdin
    // EOF) without ever sending `close`.
    if (opened) {
      await api.shutdown().catch(() => {});
    }
  }
}

async function handleOperation(operation, args, opened) {
  if (typeof operation !== "string") {
    return {
      opened,
      frame: {
        error: {
          code: "bad-request",
          message: "request is missing a string `operation`",
          fatal: false,
        },
      },
    };
  }

  if (operation === "open") {
    return await runOpen();
  }

  if (!opened) {
    return {
      opened,
      frame: {
        error: {
          code: "session-not-open",
          message: "no `open` operation has succeeded yet",
          fatal: true,
        },
      },
    };
  }

  if (operation === "close") {
    try {
      const syncError = await closeBudget();
      return {
        opened,
        frame: syncError
          ? { error: { code: "sync-failed", message: syncError, fatal: false } }
          : { ok: {} },
      };
    } catch (err) {
      return {
        opened,
        frame: {
          error: {
            code: "unhandled",
            message: describeError(err),
            fatal: true,
          },
        },
      };
    }
  }

  const handler = OPERATIONS[operation];
  if (!handler) {
    return {
      opened,
      frame: {
        error: {
          code: "unknown-operation",
          message: `unknown operation: ${operation}`,
          fatal: true,
        },
      },
    };
  }

  try {
    const result = await handler(args);
    if (WRITE_OPERATIONS.has(operation)) {
      try {
        await api.sync();
      } catch (err) {
        return {
          opened,
          frame: {
            error: {
              code: "sync-failed",
              message: `write succeeded locally but failed to sync to the server: ${describeError(err)}`,
              fatal: true,
            },
          },
        };
      }
    }
    return { opened, frame: { ok: result } };
  } catch (err) {
    if (err && err.__bridge) {
      return {
        opened,
        frame: {
          error: {
            code: err.__bridge.code,
            message: err.__bridge.message,
            fatal: false,
          },
        },
      };
    }
    return {
      opened,
      frame: {
        error: { code: "unhandled", message: describeError(err), fatal: true },
      },
    };
  }
}

async function runOpen() {
  try {
    const accounts = await openBudget();
    return { opened: true, frame: { ok: accountsToResult(accounts) } };
  } catch (err) {
    const code = err && err.__bridge ? err.__bridge.code : "unhandled";
    const message =
      err && err.__bridge ? err.__bridge.message : describeError(err);
    // `opened: true` even on failure: if `api.init()` got far enough to need
    // cleanup, `serve()`'s `finally` must still call `api.shutdown()`.
    // `shutdown()` no-ops harmlessly if `init()` never actually ran.
    return { opened: true, frame: { error: { code, message, fatal: true } } };
  }
}

async function openBudget() {
  const serverURL = mustEnvOrThrow("ACTUAL_SERVER_URL");
  const password = mustEnvOrThrow("ACTUAL_PASSWORD");
  const syncId = mustEnvOrThrow("ACTUAL_SYNC_ID");
  const e2ePassword = process.env.ACTUAL_E2E_PASSWORD || undefined;
  const dataDir = mustEnvOrThrow("ACTUAL_DATA_DIR");

  fs.mkdirSync(dataDir, { recursive: true });

  await api.init({ dataDir, serverURL, password, verbose: false });
  if (e2ePassword) {
    await api.downloadBudget(syncId, { password: e2ePassword });
  } else {
    await api.downloadBudget(syncId);
  }

  return await verifyBudgetOpen();
}

async function closeBudget() {
  let syncError = null;
  try {
    await api.sync();
  } catch (err) {
    syncError = `final sync failed: ${describeError(err)}`;
  }
  await api.shutdown();
  return syncError;
}

function mustEnvOrThrow(name) {
  const v = process.env[name];
  if (!v) throwApi("missing-env", `${name} env var is required`);
  return v;
}

// ---------------------------------------------------------------------------
// Single-shot mode (legacy)
// ---------------------------------------------------------------------------

async function main() {
  const subcommand = process.argv[2];
  const jsonFlagIdx = process.argv.indexOf("--json");
  if (!subcommand || jsonFlagIdx < 0 || !process.argv[jsonFlagIdx + 1]) {
    emitError(
      "bad-invocation",
      "usage: index.js <subcommand> --json '<args-json>'",
    );
    process.exit(1);
  }

  let args;
  try {
    args = JSON.parse(process.argv[jsonFlagIdx + 1]);
  } catch (err) {
    emitError(
      "bad-args-json",
      `--json argument was not valid JSON: ${err.message}`,
    );
    process.exit(1);
  }

  const serverURL = mustEnv("ACTUAL_SERVER_URL");
  const password = mustEnv("ACTUAL_PASSWORD");
  const syncId = mustEnv("ACTUAL_SYNC_ID");
  const e2ePassword = process.env.ACTUAL_E2E_PASSWORD || undefined;
  const dataDir = mustEnv("ACTUAL_DATA_DIR");

  fs.mkdirSync(dataDir, { recursive: true });

  await api.init({ dataDir, serverURL, password, verbose: false });
  try {
    if (e2ePassword) {
      await api.downloadBudget(syncId, { password: e2ePassword });
    } else {
      await api.downloadBudget(syncId);
    }

    const accounts = await verifyBudgetOpen();

    let result;
    switch (subcommand) {
      case "list-accounts":
        result = accountsToResult(accounts);
        break;
      case "get-balance":
        result = await getBalance(args);
        break;
      case "add-transaction":
        result = await addTransaction(args);
        break;
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
      default:
        emitError("unknown-subcommand", `unknown subcommand: ${subcommand}`);
        process.exit(1);
    }

    writeRaw(JSON.stringify(result) + "\n");
  } finally {
    await api.shutdown();
  }
}

function mustEnv(name) {
  const v = process.env[name];
  if (!v) {
    emitError("missing-env", `${name} env var is required`);
    process.exit(1);
  }
  return v;
}

function emitError(code, message) {
  writeRaw(JSON.stringify({ error: { code, message } }) + "\n");
}

// ---------------------------------------------------------------------------
// Shared operation handlers
// ---------------------------------------------------------------------------

async function verifyBudgetOpen() {
  try {
    return await api.getAccounts();
  } catch (err) {
    if (isBudgetNotOpenError(err)) {
      throwApi(
        "budget-not-open",
        "Actual reported no budget file open right after downloadBudget() " +
          "succeeded. This usually means the installed @actual-app/api " +
          "version is older than the server's budget file (a migration " +
          "mismatch) and download-budget silently swallowed the load " +
          "error. Try upgrading @actual-app/api in crates/actual/bridge " +
          "to match the server version.",
      );
    }
    throw err;
  }
}

function isBudgetNotOpenError(err) {
  const message = err && err.message;
  return (
    typeof message === "string" && message.includes("No budget file is open")
  );
}

function accountsToResult(accounts) {
  return {
    accounts: accounts.map((a) => ({
      id: a.id,
      name: a.name,
      offbudget: !!a.offbudget,
      closed: !!a.closed,
    })),
  };
}

async function getBalance({ accountId }) {
  if (!accountId) {
    throwApi("missing-account-id", "accountId is required");
  }
  // `getAccountBalance` returns integer cents.
  const balance = await api.getAccountBalance(accountId);
  return { balance: Number(balance) };
}

async function addTransaction({ accountId, amount, payeeName, notes, date }) {
  if (!accountId) {
    throwApi("missing-account-id", "accountId is required");
  }
  if (typeof amount !== "number" || !Number.isInteger(amount)) {
    throwApi("bad-amount", "amount must be an integer (cents)");
  }

  const id = crypto.randomUUID();
  const tx = {
    id,
    account: accountId,
    amount,
    date: date || new Date().toISOString().slice(0, 10),
    notes: notes || undefined,
    payee_name: payeeName || undefined,
  };
  await api.addTransactions(accountId, [tx]);
  return { id };
}

async function getLastTransaction({ accountId }) {
  if (!accountId) {
    throwApi("missing-account-id", "accountId is required");
  }
  const data = await api.runQuery(
    api
      .q("transactions")
      .filter({ account: accountId })
      .select(["date", "amount"])
      .orderBy({ date: "desc" })
      .limit(1)
      .options({ splits: "grouped" }),
  );
  if (!data.data.length) {
    throwApi(
      "no-transactions",
      `no transactions found for account ${accountId}`,
    );
  }
  const tx = data.data[0];
  return { date: tx.date, amount: Number(tx.amount) };
}

async function getBalanceAt({ accountId, date }) {
  if (!accountId) throwApi("missing-account-id", "accountId is required");
  if (!date) throwApi("missing-date", "date is required");
  const data = await api.runQuery(
    api
      .q("transactions")
      .filter({ account: accountId, date: { $lt: date } })
      .calculate({ $sum: "$amount" })
      .options({ splits: "grouped" }),
  );
  return { balance: Number(data.data) };
}

async function ensurePayee({ name }) {
  if (!name) throwApi("missing-name", "name is required");
  const payees = await api.getPayees();
  let payee = payees.find((p) => p.name === name);
  if (!payee) {
    const id = await api.createPayee({ name });
    return { id: String(id) };
  }
  return { id: String(payee.id) };
}

async function importTransaction({
  accountId,
  date,
  payeeId,
  amount,
  notes,
  cleared,
}) {
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
  if (result && Array.isArray(result.errors) && result.errors.length > 0) {
    throwApi(
      "transaction-not-created",
      result.errors.map((e) => e.message).join("; "),
    );
  }
  const id =
    result &&
    ((result.added && result.added[0]) ||
      (result.updated && result.updated[0]));
  if (!id) {
    throwApi("transaction-not-created", "Actual returned no transaction id");
  }
  return { id: String(id) };
}

function throwApi(code, message) {
  const err = new Error(message);
  err.__bridge = { code, message };
  throw err;
}

function describeError(err) {
  if (err && typeof err.stack === "string") {
    return err.stack;
  }
  if (err && typeof err === "object") {
    try {
      return JSON.stringify(err);
    } catch {
      // fall through to String() below, e.g. circular structures
    }
  }
  return String(err);
}
