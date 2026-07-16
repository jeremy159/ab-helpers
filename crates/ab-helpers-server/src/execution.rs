use async_trait::async_trait;

/// Marker struct for dry-run mode - read-only, no writes performed.
#[derive(Debug, Clone, Copy)]
pub struct DryRun;

/// Marker struct for live mode - writes are performed.
#[derive(Debug, Clone, Copy)]
pub struct Live;

#[doc(hidden)]
pub mod private {
    pub trait ExecutionMode {}
}

impl private::ExecutionMode for DryRun {}
impl private::ExecutionMode for Live {}

/// Result of the plan (read) phase. Either an early exit or a ready-to-execute plan.
pub enum PlanOutcome<S, P> {
    /// An early-exit condition - no work to do. The service defines what `S` contains.
    Skip(S),
    /// Plan is ready; proceed to the apply phase.
    Ready(P),
}

/// A service that exposes the plan/apply execution pattern for CLI-facing commands
/// that support `--dry-run`. Only implement this for services invoked from the CLI.
///
/// Implementors provide two methods; `run<M>()` is derived for free.
#[async_trait]
pub trait PlanExecute: Sized + Send + Sync {
    /// Early-exit reasons (e.g. account closed, nothing to do).
    type Skip: Send;
    /// Intermediate state produced by the read-only plan phase.
    type Plan: Send;
    /// The write capability used by the apply phase (typically the client type).
    type Writer: Send + Sync;

    fn writer(&self) -> &Self::Writer;

    async fn plan(&self) -> anyhow::Result<PlanOutcome<Self::Skip, Self::Plan>>;

    /// Single public entry point. Call as `service.run::<DryRun>()` or `service.run::<Live>()`.
    /// Do not override this method.
    async fn run<M>(&self) -> anyhow::Result<M::Outcome>
    where
        M: RunMode<Self::Skip, Self::Plan, Self::Writer> + Send + Sync,
        M::Outcome: Send,
    {
        match self.plan().await? {
            PlanOutcome::Skip(reason) => Ok(M::on_skip(reason)),
            PlanOutcome::Ready(plan) => M::apply(self.writer(), plan).await,
        }
    }
}

/// Encodes the mode-specific behaviour for plan/apply execution.
///
/// - `S` - the skip/early-exit reason type
/// - `P` - the plan (intermediate read-phase result) type
/// - `W` - the write capability type (the concrete constraint on W is on the impl, not here)
#[async_trait]
pub trait RunMode<S, P, W>: private::ExecutionMode + Send + Sync {
    type Outcome: Send;

    /// Convert an early-exit reason into this mode's outcome type.
    fn on_skip(reason: S) -> Self::Outcome;

    /// Mode-specific apply step.
    /// `DryRun` implementations ignore `writer` and project the plan.
    /// `Live` implementations call `writer` for mutations.
    async fn apply(writer: &W, plan: P) -> anyhow::Result<Self::Outcome>
    where
        W: Send + Sync;
}
