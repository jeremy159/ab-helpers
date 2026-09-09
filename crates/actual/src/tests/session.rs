use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader as TokioBufReader};

use crate::{BridgeInvoker, BridgeSession, BridgeTimeouts, Error};

fn timeouts() -> BridgeTimeouts {
    BridgeTimeouts {
        open: Duration::from_secs(2),
        operation: Duration::from_secs(2),
        close: Duration::from_secs(2),
        lock: Duration::from_secs(2),
    }
}

/// Spawns a fake "server" task that reads NDJSON requests from `server`
/// and responds according to `respond`. `respond` returns `None` to
/// simulate the process exiting without a response (EOF).
fn spawn_fake_server<F>(server: tokio::io::DuplexStream, respond: F)
where
    F: Fn(Value) -> Option<Value> + Send + 'static,
{
    tokio::spawn(async move {
        let mut reader = TokioBufReader::new(server);
        let mut line = String::new();
        loop {
            line.clear();
            let n = reader.read_line(&mut line).await.unwrap_or(0);
            if n == 0 {
                break;
            }
            let req: Value = match serde_json::from_str(line.trim()) {
                Ok(v) => v,
                Err(_) => continue,
            };
            match respond(req) {
                Some(resp) => {
                    let out = format!("{}\n", serde_json::to_string(&resp).unwrap());
                    if reader.write_all(out.as_bytes()).await.is_err() {
                        break;
                    }
                    if reader.flush().await.is_err() {
                        break;
                    }
                }
                None => break,
            }
        }
    });
}

fn make_session(respond: impl Fn(Value) -> Option<Value> + Send + 'static) -> BridgeSession {
    let (client_side, server_side) = tokio::io::duplex(8192);
    let (read_half, write_half) = tokio::io::split(client_side);
    spawn_fake_server(server_side, respond);
    BridgeSession::from_io(write_half, read_half, timeouts())
}

fn ok_frame(id: &Value, payload: Value) -> Value {
    serde_json::json!({ "id": id, "ok": payload })
}

#[tokio::test]
async fn happy_path_open_call_close() {
    let session = make_session(|req| {
        let id = req.get("id").cloned().unwrap();
        let operation = req.get("operation").and_then(Value::as_str).unwrap_or_default();
        Some(match operation {
            "open" => ok_frame(&id, serde_json::json!({"accounts": []})),
            "list-accounts" => ok_frame(&id, serde_json::json!({"accounts": [{"id":"a1"}]})),
            "close" => ok_frame(&id, serde_json::json!({})),
            _ => serde_json::json!({"id": id, "error": {"code":"unknown-operation","message":"?","fatal":true}}),
        })
    });

    let accounts = session
        .invoke("list-accounts", Value::Object(Default::default()))
        .await
        .unwrap();
    assert_eq!(accounts["accounts"][0]["id"], "a1");

    session.close().await.unwrap();
    // Idempotent.
    session.close().await.unwrap();
}

#[tokio::test]
async fn recoverable_error_leaves_session_usable() {
    let session = make_session(|req| {
        let id = req.get("id").cloned().unwrap();
        let operation = req.get("operation").and_then(Value::as_str).unwrap_or_default();
        Some(match operation {
            "get-balance" => {
                serde_json::json!({"id": id, "error": {"code":"missing-account-id","message":"accountId is required","fatal":false}})
            }
            "list-accounts" => ok_frame(&id, serde_json::json!({"accounts": []})),
            _ => ok_frame(&id, serde_json::json!({})),
        })
    });

    let err = session
        .invoke("get-balance", Value::Object(Default::default()))
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Api(ref e) if e.code == "missing-account-id"));

    // Session must still be usable after a non-fatal error.
    let ok = session
        .invoke("list-accounts", Value::Object(Default::default()))
        .await
        .unwrap();
    assert_eq!(ok["accounts"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn fatal_error_poisons_session() {
    let session = make_session(|req| {
        let id = req.get("id").cloned().unwrap();
        Some(serde_json::json!({"id": id, "error": {"code":"budget-not-open","message":"nope","fatal":true}}))
    });

    let err = session
        .invoke("list-accounts", Value::Object(Default::default()))
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Api(ref e) if e.fatal));

    // Session must now fail fast without touching the pipe.
    let err2 = session
        .invoke("list-accounts", Value::Object(Default::default()))
        .await
        .unwrap_err();
    assert!(matches!(err2, Error::SessionClosed(_)));
}

#[tokio::test]
async fn id_mismatch_poisons_session() {
    let session = make_session(|req| {
        let id = req.get("id").and_then(Value::as_u64).unwrap_or(0);
        // Always reply with the wrong id.
        Some(serde_json::json!({"id": id + 100, "ok": {}}))
    });

    let err = session
        .invoke("list-accounts", Value::Object(Default::default()))
        .await
        .unwrap_err();
    assert!(matches!(err, Error::BridgeProtocol(_)));
}

#[tokio::test]
async fn eof_mid_operation_is_bridge_protocol_error() {
    let session = make_session(|_req| None);

    let err = session
        .invoke("list-accounts", Value::Object(Default::default()))
        .await
        .unwrap_err();
    assert!(matches!(err, Error::BridgeProtocol(_)));
}

#[tokio::test]
async fn timeout_poisons_session() {
    let mut t = timeouts();
    t.operation = Duration::from_millis(50);
    let (client_side, server_side) = tokio::io::duplex(8192);
    let (read_half, write_half) = tokio::io::split(client_side);
    // Fake server that never responds.
    tokio::spawn(async move {
        let mut reader = TokioBufReader::new(server_side);
        let mut line = String::new();
        let _ = reader.read_line(&mut line).await;
        // Hold the connection open without responding.
        tokio::time::sleep(Duration::from_secs(5)).await;
    });
    let session = BridgeSession::from_io(write_half, read_half, t);

    let err = session
        .invoke("list-accounts", Value::Object(Default::default()))
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Timeout { .. }));

    let err2 = session
        .invoke("list-accounts", Value::Object(Default::default()))
        .await
        .unwrap_err();
    assert!(matches!(err2, Error::SessionClosed(_)));
}

#[tokio::test]
async fn close_on_never_used_session_is_ok() {
    let session = make_session(|req| {
        let id = req.get("id").cloned().unwrap();
        Some(ok_frame(&id, serde_json::json!({})))
    });
    session.close().await.unwrap();
}
