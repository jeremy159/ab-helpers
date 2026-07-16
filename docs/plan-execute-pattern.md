# Plan-Execute Pattern

Architecture for CLI-exposed services that support `--dry-run` mode.

## When to Use

Implement `PlanExecute` only when a service is invoked directly from the CLI and needs to support `--dry-run`. Services that are purely internal (called only from other server-side code, not exposed as CLI commands) have no use for this pattern — add it when the CLI surfaces it, not speculatively.

---

## Core Idea

Split execution into two named phases, named after Terraform's canonical model:

- **`plan`** — read-only. Gathers all state and computes what would happen. Returns an intermediate `Plan` struct.
- **`apply`** — write phase. Takes the plan and performs the mutations. Only called in live mode.

A single public entry point `run<M: RunMode>()` calls both phases. Dry-run mode's `apply` implementation is a no-op projection of the plan — writes are structurally absent, not guarded by a runtime flag.

---

## Traits (in `ab-helpers-server`)

The execution kernel (`PlanExecute`, `RunMode`, `DryRun`, `Live`, `PlanOutcome`) lives in `ab-helpers-server`, not `ab-helpers-domain`. The dry-run/live distinction is about *whether we touch the outside world* — an infrastructure concern — and the `Live` apply phase writes through the Actual client, which only the server crate knows about. Keeping the kernel here lets the `RunMode` impls call the client directly with no bridging adapter. The domain crate holds only the data vocabulary (`Skip`/`Plan`/outcome types) and the pure calculations.

### `PlanExecute`

```rust
pub trait PlanExecute: Sized + Send + Sync {
    /// Early-exit reasons: cases where no work is needed (service-defined).
    type Skip: Send;
    /// Intermediate state produced by the read phase (a domain data type).
    type Plan: Send;
    /// The write capability the apply phase requires (usually the Actual client type).
    type Writer: Send + Sync;

    fn writer(&self) -> &Self::Writer;
    async fn plan(&self) -> ABHelpersResult<PlanOutcome<Self::Skip, Self::Plan>>;

    // Default implementation — do not override.
    async fn run<M>(&self) -> ABHelpersResult<M::Outcome>
    where
        M: RunMode<Self::Skip, Self::Plan, Self::Writer>,
    {
        match self.plan().await? {
            PlanOutcome::Skip(reason) => Ok(M::on_skip(reason)),
            PlanOutcome::Ready(plan)  => M::apply(self.writer(), plan).await,
        }
    }
}
```

### `PlanOutcome<S, P>`

```rust
pub enum PlanOutcome<S, P> {
    Skip(S),   // early exit — no work to do
    Ready(P),  // proceed to apply phase
}
```

### `RunMode<S, P, W>`

```rust
pub trait RunMode<S, P, W>: sealed::RunMode + Send + Sync {
    type Outcome: Send;

    /// Map an early-exit reason into this mode's outcome type.
    fn on_skip(reason: S) -> Self::Outcome;

    /// The mode-specific apply step.
    /// `DryRun` ignores `writer` and projects the plan.
    /// `Live` calls writer for the actual mutations.
    async fn apply(writer: &W, plan: P) -> ABHelpersResult<Self::Outcome>;
}
```

`DryRun` and `Live` are sealed marker structs — no external implementations.

---

## Implementing a CLI-Facing Service

### 1. Define data types in `ab-helpers-domain`

The domain crate holds only the data vocabulary — no behaviour, no client access. That includes the early-exit reasons, the per-mode outcomes, and the plan struct (the read-phase intermediate). They're plain data shared with the CLI for matching.

```rust
// Early-exit reasons
pub enum FooSkip {
    SomeReason { ... },
}

// Outcome types (one per mode)
pub enum FooDryRunOutcome {
    Skip(FooSkip),
    WouldApply { ... },
}

pub enum FooLiveOutcome {
    Skip(FooSkip),
    Applied { ... },
}

// Read-phase intermediate
pub struct FooPlan { /* all fields computed during the read phase */ }
```

### 2. Implement the `RunMode` impls and `PlanExecute` in `ab-helpers-server`

The `RunMode` impls and `PlanExecute` impl live in the server crate, alongside the service. The `Live` apply phase calls the Actual client directly — no `InterestApplier`-style adapter is needed, because `RunMode` is a local trait here so the impls can name `actual` types and map errors in place.

```rust
// RunMode impls — these touch the client, so they live with the server.
impl<C: FooClientBound> RunMode<FooSkip, FooPlan, C> for DryRun {
    type Outcome = FooDryRunOutcome;

    fn on_skip(reason: FooSkip) -> FooDryRunOutcome { FooDryRunOutcome::Skip(reason) }

    async fn apply(_writer: &C, plan: FooPlan) -> ABHelpersResult<FooDryRunOutcome> {
        // No writes. Project plan fields into outcome.
        Ok(FooDryRunOutcome::WouldApply { ... })
    }
}

impl<C: FooClientBound> RunMode<FooSkip, FooPlan, C> for Live {
    type Outcome = FooLiveOutcome;

    fn on_skip(reason: FooSkip) -> FooLiveOutcome { FooLiveOutcome::Skip(reason) }

    async fn apply(writer: &C, plan: FooPlan) -> ABHelpersResult<FooLiveOutcome> {
        // Writes happen here and only here — call the client directly.
        let result = writer.some_write_op(...).await?;
        Ok(FooLiveOutcome::Applied { ... })
    }
}

impl<C: FooClientBound> PlanExecute for FooService<C> {
    type Skip   = FooSkip;
    type Plan   = FooPlan;
    type Writer = C;

    fn writer(&self) -> &C { &self.client }

    async fn plan(&self) -> ABHelpersResult<PlanOutcome<FooSkip, FooPlan>> {
        // Read-only operations. Return Skip(...) or Ready(FooPlan { ... }).
    }
}
// run<M>() is provided by the default impl — no override needed.
```

### 3. Call from CLI

```rust
if args.dry_run {
    service.run::<DryRun>().await
} else {
    service.run::<Live>().await
}
```