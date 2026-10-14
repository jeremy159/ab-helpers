pub mod error;

use std::io::IsTerminal;

pub mod budget_details;
pub mod budget_summary;
pub mod daemon;
pub mod init;
pub mod interest;
pub mod list_accounts;
pub mod set_balance;

#[cfg(test)]
mod tests;

pub(crate) fn print_warning(message: &str) {
    if std::io::stdout().is_terminal() {
        println!("\x1b[33m⚠ {message}\x1b[0m");
    } else {
        println!("⚠ {message}");
    }
}
