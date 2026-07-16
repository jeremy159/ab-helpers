mod interest;
mod reconcile;
#[cfg(test)]
mod tests;

pub use interest::*;
pub use reconcile::*;

pub trait ActualClient:
    actual::AccountRequests + actual::TransactionRequests + Send + Sync
{
}

impl<T> ActualClient for T where
    T: actual::AccountRequests + actual::TransactionRequests + Send + Sync
{
}
