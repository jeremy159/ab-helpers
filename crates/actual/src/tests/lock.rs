use std::time::Duration;

use crate::{error::Error, lock::BudgetLock};

#[tokio::test]
async fn acquires_and_releases_on_drop() {
    let dir = tempfile::tempdir().unwrap();
    let lock = BudgetLock::acquire(dir.path(), Duration::from_millis(50))
        .await
        .unwrap();
    drop(lock);
    // Should be immediately re-acquirable now that it was dropped.
    BudgetLock::acquire(dir.path(), Duration::from_millis(50))
        .await
        .unwrap();
}

#[tokio::test]
async fn second_acquire_times_out_while_first_is_held() {
    let dir = tempfile::tempdir().unwrap();
    let _first = BudgetLock::acquire(dir.path(), Duration::from_millis(50))
        .await
        .unwrap();

    let err = BudgetLock::acquire(dir.path(), Duration::from_millis(200))
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Bridge(_)));
}

#[tokio::test]
async fn second_acquire_succeeds_once_first_is_dropped_mid_wait() {
    let dir = tempfile::tempdir().unwrap();
    let first = BudgetLock::acquire(dir.path(), Duration::from_millis(50))
        .await
        .unwrap();

    let path = dir.path().to_path_buf();
    let waiter =
        tokio::spawn(async move { BudgetLock::acquire(&path, Duration::from_secs(5)).await });

    tokio::time::sleep(Duration::from_millis(100)).await;
    drop(first);

    waiter.await.unwrap().unwrap();
}
