mod account_match;
mod budget_data;
mod budget_details;
mod budget_summary;
mod income_schedule_adapter;
mod interest;
mod payee_match;
mod reconcile;
#[cfg(test)]
mod tests;

pub use account_match::*;
pub use budget_details::*;
pub use budget_summary::*;
pub use interest::*;
pub use payee_match::*;
pub use reconcile::*;

/// Blanket alias for types that can only read from Actual.
pub trait ActualReadClient: actual::ActualReadRequests + Send + Sync {}
impl<T: actual::ActualReadRequests + Send + Sync> ActualReadClient for T {}

/// Blanket alias for types that can only write to Actual.
pub trait ActualWriteClient: actual::ActualWriteRequests + Send + Sync {}
impl<T: actual::ActualWriteRequests + Send + Sync> ActualWriteClient for T {}

/// Blanket alias for types that can both read and write.
pub trait ActualClient: ActualReadClient + ActualWriteClient {}
impl<T: ActualReadClient + ActualWriteClient> ActualClient for T {}
