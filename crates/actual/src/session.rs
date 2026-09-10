//! A long-lived Node bridge process reused across every call that makes up
//! one logical operation (one CLI command, one daemon tick), instead of
//! spawning a fresh `node` process - and re-downloading/re-syncing the whole
//! local budget replica - per call.
//!
//! [`BridgeSession`] implements the same [`BridgeInvoker`] trait as
//! [`BridgeConfig`], so [`crate::Client`] needs no changes to use one: build
//! a `Client` with [`crate::Client::with_invoker`] over an
//! `Arc<BridgeSession>`.

use std::collections::VecDeque;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use async_trait::async_trait;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

use crate::ActualResult;
use crate::bridge::{BridgeConfig, BridgeInvoker, BridgeRequest, BridgeTimeouts};
use crate::error::{ApiError, Error};
use crate::lock::BudgetLock;

const STDERR_TAIL_CAPACITY: usize = 64;

enum State {
    Open,
    Closed,
    /// Session is no longer usable; carries a human-readable reason.
    Poisoned(String),
}

struct SessionIo {
    state: State,
    /// `None` for the in-memory test seam ([`BridgeSession::from_io`]).
    child: Option<Child>,
    stdin: Box<dyn AsyncWrite + Send + Unpin>,
    stdout: Lines<BufReader<Box<dyn AsyncRead + Send + Unpin>>>,
    stderr_tail: Arc<StdMutex<VecDeque<String>>>,
}

/// One open `node index.js serve` process with one open Actual budget.
///
/// Strictly one request in flight at a time (enforced by the internal
/// mutex), FIFO. Implements [`BridgeInvoker`], so an existing [`crate::Client`]
/// can be built directly on top of it.
pub struct BridgeSession {
    io: Mutex<SessionIo>,
    next_id: AtomicU64,
    timeouts: BridgeTimeouts,
    _lock: Option<BudgetLock>,
}

impl BridgeConfig {
    /// Spawns `node <bridge_script> serve`, acquires the data-dir lock, and
    /// performs the `open` handshake (`api.init` + `downloadBudget` +
    /// verification that the budget actually loaded).
    pub async fn open_session(&self) -> ActualResult<BridgeSession> {
        let lock = BudgetLock::acquire(&self.cache_dir, self.timeouts.lock).await?;

        let mut cmd = Command::new(&self.node_bin);
        cmd.arg(&self.bridge_script)
            .arg("serve")
            .envs(self.env())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let mut child = cmd.spawn().map_err(|e| {
            Error::Bridge(format!(
                "failed to spawn `{} {} serve`: {e}",
                self.node_bin.display(),
                self.bridge_script.display()
            ))
        })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| Error::Bridge("bridge child has no stdin".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::Bridge("bridge child has no stdout".to_string()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| Error::Bridge("bridge child has no stderr".to_string()))?;

        let stderr_tail = Arc::new(StdMutex::new(VecDeque::with_capacity(STDERR_TAIL_CAPACITY)));
        spawn_stderr_drain(stderr, Arc::clone(&stderr_tail));

        let session = BridgeSession {
            io: Mutex::new(SessionIo {
                state: State::Open,
                child: Some(child),
                stdin: Box::new(stdin),
                stdout: BufReader::new(Box::new(stdout) as Box<dyn AsyncRead + Send + Unpin>)
                    .lines(),
                stderr_tail,
            }),
            next_id: AtomicU64::new(1),
            timeouts: self.timeouts,
            _lock: Some(lock),
        };

        session.invoke(BridgeRequest::Open).await?;

        Ok(session)
    }
}

fn spawn_stderr_drain(
    stderr: impl AsyncRead + Send + Unpin + 'static,
    tail: Arc<StdMutex<VecDeque<String>>>,
) {
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    tracing::debug!(target: "actual::bridge::stderr", "{line}");
                    let mut guard = tail.lock().unwrap_or_else(|e| e.into_inner());
                    if guard.len() >= STDERR_TAIL_CAPACITY {
                        guard.pop_front();
                    }
                    guard.push_back(line);
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }
    });
}

fn stderr_suffix(tail: &StdMutex<VecDeque<String>>) -> String {
    let guard = tail.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_empty() {
        String::new()
    } else {
        let joined = guard.iter().cloned().collect::<Vec<_>>().join("\n");
        format!("\nstderr:\n{joined}")
    }
}

impl BridgeSession {
    /// Test-only seam: drives the protocol over arbitrary in-memory IO
    /// instead of a real child process, so it can be unit-tested without
    /// Node or a live Actual server. No data-dir lock is taken.
    #[cfg(test)]
    pub(crate) fn from_io<W, R>(stdin: W, stdout: R, timeouts: BridgeTimeouts) -> Self
    where
        W: AsyncWrite + Send + Unpin + 'static,
        R: AsyncRead + Send + Unpin + 'static,
    {
        Self {
            io: Mutex::new(SessionIo {
                state: State::Open,
                child: None,
                stdin: Box::new(stdin),
                stdout: BufReader::new(Box::new(stdout) as Box<dyn AsyncRead + Send + Unpin>)
                    .lines(),
                stderr_tail: Arc::new(StdMutex::new(VecDeque::new())),
            }),
            next_id: AtomicU64::new(1),
            timeouts,
            _lock: None,
        }
    }

    /// Sends `close` (the bridge syncs once more and shuts the budget down),
    /// then waits for the child to exit. Idempotent: closing twice, or
    /// closing an already-poisoned session, is safe.
    pub async fn close(&self) -> ActualResult<()> {
        {
            let mut io = self.io.lock().await;
            match &io.state {
                State::Closed => return Ok(()),
                State::Poisoned(reason) => {
                    let reason = reason.clone();
                    if let Some(child) = io.child.as_mut() {
                        let _ = child.start_kill();
                        let _ = child.wait().await;
                    }
                    io.state = State::Closed;
                    return Err(Error::SessionClosed(reason));
                }
                State::Open => {}
            }
        }

        let result = self.invoke(BridgeRequest::Close).await;

        let mut io = self.io.lock().await;
        if let Some(child) = io.child.as_mut()
            && tokio::time::timeout(self.timeouts.close, child.wait())
                .await
                .is_err()
        {
            // The bridge didn't exit on its own within the timeout (e.g. it
            // answered `close` but got stuck tearing down afterwards) - kill
            // it rather than waiting unboundedly.
            let _ = child.start_kill();
            let _ = child.wait().await;
        }
        io.state = State::Closed;
        result.map(|_| ())
    }
}

#[async_trait]
impl BridgeInvoker for BridgeSession {
    async fn invoke(&self, request: BridgeRequest) -> ActualResult<Value> {
        let mut io = self.io.lock().await;

        match &io.state {
            State::Closed => {
                return Err(Error::SessionClosed("session already closed".to_string()));
            }
            State::Poisoned(reason) => {
                return Err(Error::SessionClosed(reason.clone()));
            }
            State::Open => {}
        }

        let id = self.next_id.fetch_add(1, Ordering::Relaxed);

        let timeout = match request {
            BridgeRequest::Open => self.timeouts.open,
            BridgeRequest::Close => self.timeouts.close,
            _ => self.timeouts.operation,
        };

        let (operation, args) = request.wire_parts()?;
        let envelope = serde_json::json!({ "id": id, "operation": operation, "args": args });
        let line = serde_json::to_string(&envelope)?;

        tracing::debug!(%operation, id, "invoking actual bridge session");

        let round_trip = tokio::time::timeout(timeout, async {
            write_line(&mut *io.stdin, &line).await?;
            io.stdout.next_line().await
        })
        .await;

        let response_line = match round_trip {
            Err(_elapsed) => {
                let reason = format!(
                    "operation `{operation}` timed out after {timeout:?}{}",
                    stderr_suffix(&io.stderr_tail)
                );
                io.state = State::Poisoned(reason);
                if let Some(child) = io.child.as_mut() {
                    let _ = child.start_kill();
                }
                return Err(Error::Timeout {
                    operation: operation.to_string(),
                    secs: timeout.as_secs(),
                });
            }
            Ok(Err(e)) => {
                let reason = format!(
                    "bridge I/O failed while sending `{operation}`: {e}{}",
                    stderr_suffix(&io.stderr_tail)
                );
                io.state = State::Poisoned(reason.clone());
                return Err(Error::Bridge(reason));
            }
            Ok(Ok(None)) => {
                let status = if let Some(child) = io.child.as_mut() {
                    child.wait().await.ok()
                } else {
                    None
                };
                let reason = format!(
                    "bridge process ended unexpectedly (status: {status:?}){}",
                    stderr_suffix(&io.stderr_tail)
                );
                io.state = State::Poisoned(reason.clone());
                return Err(Error::BridgeProtocol(reason));
            }
            Ok(Ok(Some(line))) => line,
        };

        let frame: Value = match serde_json::from_str(&response_line) {
            Ok(v) => v,
            Err(e) => {
                let reason = format!(
                    "could not parse bridge response as JSON: {e}\nline: {response_line}{}",
                    stderr_suffix(&io.stderr_tail)
                );
                io.state = State::Poisoned(reason.clone());
                return Err(Error::BridgeProtocol(reason));
            }
        };

        let resp_id = frame.get("id").and_then(Value::as_u64);
        if resp_id != Some(id) {
            let reason = format!(
                "bridge response id mismatch: sent {id}, got {resp_id:?} (stream desynchronised){}",
                stderr_suffix(&io.stderr_tail)
            );
            io.state = State::Poisoned(reason.clone());
            return Err(Error::BridgeProtocol(reason));
        }

        if let Some(err_obj) = frame.get("error") {
            let api_err: ApiError = serde_json::from_value(err_obj.clone())?;
            if api_err.fatal {
                io.state = State::Poisoned(api_err.message.clone());
            }
            return Err(Error::Api(api_err));
        }

        Ok(frame.get("ok").cloned().unwrap_or(Value::Null))
    }
}

async fn write_line<W: AsyncWrite + Unpin + ?Sized>(w: &mut W, line: &str) -> std::io::Result<()> {
    w.write_all(line.as_bytes()).await?;
    w.write_all(b"\n").await?;
    w.flush().await
}
