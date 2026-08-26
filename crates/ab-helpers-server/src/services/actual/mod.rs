mod interest;
mod reconcile;
#[cfg(test)]
mod tests;

pub use interest::*;
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
