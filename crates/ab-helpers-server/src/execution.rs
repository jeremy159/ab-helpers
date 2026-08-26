use async_trait::async_trait;

use crate::error::ABHelpersResult;

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

/// Output of a dry-run execution: either an early skip or the computed plan, unchanged.
///
/// Because `DryRun` returns the plan verbatim, the CLI can render any field
/// without a separate "preview outcome" type per service.
#[derive(Debug)]
pub enum Preview<S, P> {
    Skip(S),
    WouldApply(P),
}

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
/// ## Write-safety guarantee
///
/// `plan()` receives only `&Self::PlanCtx`, a read-only struct that holds the
/// reader client and configuration — the writer is not present in that type at
/// all. Write methods are therefore unreachable from `plan()` at the type level,
/// not merely by convention.
///
/// `DryRun::apply` is additionally sealed: `W` is opaque (`Send + Sync` only),
/// so no write methods can be called on the writer argument there either.
///
/// Implementors provide `plan_ctx`, `writer`, and `plan`; `run<M>()` is derived for free.
#[async_trait]
pub trait PlanExecute: Sized + Send + Sync {
    /// Early-exit reasons (e.g. account closed, nothing to do).
    type Skip: Send;
    /// Intermediate state produced by the read-only plan phase.
    type Plan: Send;
    /// Read-only context passed to `plan()`. Must NOT contain the write client.
    type PlanCtx: Send + Sync;
    /// The write capability used by the apply phase (typically the client type).
    type Writer: Send + Sync;

    fn plan_ctx(&self) -> &Self::PlanCtx;
    fn writer(&self) -> &Self::Writer;

    /// Read-only planning phase. Receives only the `PlanCtx` — the writer is
    /// structurally absent, so writes cannot occur here regardless of mode.
    async fn plan(ctx: &Self::PlanCtx) -> ABHelpersResult<PlanOutcome<Self::Skip, Self::Plan>>;

    /// Single public entry point. Call as `service.run::<DryRun>()` or `service.run::<Live>()`.
    /// Do not override this method.
    async fn run<M>(&self) -> ABHelpersResult<M::Outcome>
    where
        M: RunMode<Self::Skip, Self::Plan, Self::Writer> + Send + Sync,
        M::Outcome: Send,
    {
        match Self::plan(self.plan_ctx()).await? {
            PlanOutcome::Skip(reason) => Ok(M::on_skip(reason)),
            PlanOutcome::Ready(plan) => M::apply(self.writer(), plan).await,
        }
    }
}

/// Generic dry-run: ignores the writer entirely and returns the plan as-is.
///
/// `W` is opaque (`Send + Sync` only), so no write methods can be called on
/// the writer argument — structural enforcement for the apply phase.
#[async_trait]
impl<S: Send + 'static, P: Send + 'static, W: Send + Sync> RunMode<S, P, W> for DryRun {
    type Outcome = Preview<S, P>;

    fn on_skip(reason: S) -> Preview<S, P> {
        Preview::Skip(reason)
    }

    async fn apply(_writer: &W, plan: P) -> ABHelpersResult<Preview<S, P>> {
        Ok(Preview::WouldApply(plan))
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
    async fn apply(writer: &W, plan: P) -> ABHelpersResult<Self::Outcome>;
}
