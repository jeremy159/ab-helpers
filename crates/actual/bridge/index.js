#!/usr/bin/env node
//
// Bridge between the Rust `actual` crate and `@actual-app/api`.
//
// Protocol:
//   node index.js <subcommand> --json '<args-json>'
//
// Subcommands:
//   list-accounts     args: {}
//   get-balance       args: {accountId}
//   add-transaction   args: {accountId, amount, payeeName?, notes?, date?}
//
// Output: a single JSON line on stdout. On success, the subcommand's success
// shape. On failure, `{"error":{"code":"...","message":"..."}}` and exit code 1.
//
// All connection config is provided via env vars to keep secrets off argv:
//   ACTUAL_SERVER_URL, ACTUAL_PASSWORD, ACTUAL_SYNC_ID,
//   ACTUAL_E2E_PASSWORD (optional), ACTUAL_DATA_DIR.

"use strict";

const fs = require("fs");
const path = require("path");

let api;
try {
  api = require("@actual-app/api");
} catch (err) {
  emitError("bridge-load-failed", `failed to require @actual-app/api: ${err.message}. Did you run \`npm install\` in crates/actual/bridge?`);
  process.exit(1);
}

async function main() {
  const subcommand = process.argv[2];
  const jsonFlagIdx = process.argv.indexOf("--json");
  if (!subcommand || jsonFlagIdx < 0 || !process.argv[jsonFlagIdx + 1]) {
    emitError("bad-invocation", "usage: index.js <subcommand> --json '<args-json>'");
    process.exit(1);
  }

  let args;
  try {
    args = JSON.parse(process.argv[jsonFlagIdx + 1]);
  } catch (err) {
    emitError("bad-args-json", `--json argument was not valid JSON: ${err.message}`);
    process.exit(1);
  }

  const serverURL = mustEnv("ACTUAL_SERVER_URL");
  const password = mustEnv("ACTUAL_PASSWORD");
  const syncId = mustEnv("ACTUAL_SYNC_ID");
  const e2ePassword = process.env.ACTUAL_E2E_PASSWORD || undefined;
  const dataDir = mustEnv("ACTUAL_DATA_DIR");

  fs.mkdirSync(dataDir, { recursive: true });

  await api.init({ dataDir, serverURL, password });
  try {
    if (e2ePassword) {
      await api.downloadBudget(syncId, { password: e2ePassword });
    } else {
      await api.downloadBudget(syncId);
    }

    let result;
    switch (subcommand) {
      case "list-accounts":
        result = await listAccounts();
        break;
      case "get-balance":
        result = await getBalance(args);
        break;
      case "add-transaction":
        result = await addTransaction(args);
        break;
      default:
        emitError("unknown-subcommand", `unknown subcommand: ${subcommand}`);
        process.exit(1);
    }

    process.stdout.write(JSON.stringify(result) + "\n");
  } finally {
    await api.shutdown();
  }
}

async function listAccounts() {
  const accounts = await api.getAccounts();
  return {
    accounts: accounts.map(a => ({
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
  const tx = {
    account: accountId,
    amount,
    date: date || new Date().toISOString().slice(0, 10),
    notes: notes || undefined,
    payee_name: payeeName || undefined,
  };
  const result = await api.addTransactions(accountId, [tx]);
  // addTransactions returns an array of new ids.
  const id = Array.isArray(result) ? result[0] : (result && result.added && result.added[0]);
  if (!id) {
    throwApi("transaction-not-created", "Actual returned no transaction id");
  }
  return { id: String(id) };
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
  process.stdout.write(JSON.stringify({ error: { code, message } }) + "\n");
}

function throwApi(code, message) {
  const err = new Error(message);
  err.__bridge = { code, message };
  throw err;
}

main().catch(err => {
  if (err && err.__bridge) {
    emitError(err.__bridge.code, err.__bridge.message);
  } else {
    emitError("unhandled", err && err.stack ? err.stack : String(err));
  }
  process.exit(1);
});
