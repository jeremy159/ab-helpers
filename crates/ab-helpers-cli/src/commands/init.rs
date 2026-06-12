use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use ab_helpers_server::config;
use anyhow::{Context, bail};
use clap::Args;

/// Seed the user config dir (`~/.config/ab-helpers`) with `base.toml` (defaults)
/// and a starter `config.toml` (your overrides).
#[derive(Args, Debug)]
pub struct InitArgs {
    /// Source `configuration/` directory to copy `base.toml` from. Defaults to
    /// the dir next to the binary, then the project directory.
    #[arg(long)]
    from: Option<PathBuf>,
    /// Overwrite an existing `config.toml`.
    #[arg(long)]
    force: bool,
}

const CONFIG_STUB: &str = "\
# ab-helpers CLI overrides. Values here layer on top of base.toml; only set the
# fields you need to change.

[actual]
server_url = \"\"
password = \"\"
sync_id = \"\"

[actual.kia]
account_id = \"\"

[actual.mortgage]
account_id = \"\"
";

pub fn run(args: InitArgs) -> anyhow::Result<ExitCode> {
    tracing::info!("init command started");

    let dest = config::user_config_dir()
        .context("could not determine ~/.config/ab-helpers (is $HOME set?)")?;
    tracing::debug!(dest = %dest.display(), "resolved config directory");
    fs::create_dir_all(&dest).with_context(|| format!("failed to create {}", dest.display()))?;

    let src_dir = args
        .from
        .or_else(config::default_config_source_dir)
        .context("could not locate a source configuration directory; pass --from <dir>")?;
    let src_base = src_dir.join("base.toml");
    tracing::debug!(src = %src_base.display(), "located base.toml source");
    if !src_base.is_file() {
        tracing::error!(path = %src_base.display(), "base.toml not found in source directory");
        bail!("no base.toml in {}", src_dir.display());
    }

    // base.toml is the shipped defaults floor — always refreshed so re-running
    // `init` forwards project updates. Your overrides live in config.toml.
    let dest_base = dest.join("base.toml");
    let refreshed = dest_base.exists();
    fs::copy(&src_base, &dest_base)
        .with_context(|| format!("failed to copy base.toml to {}", dest_base.display()))?;
    tracing::debug!(path = %dest_base.display(), refreshed, "base.toml written");
    println!(
        "{} {}",
        if refreshed { "refreshed" } else { "wrote" },
        dest_base.display()
    );

    let dest_config = dest.join("config.toml");
    if dest_config.exists() && !args.force {
        tracing::warn!(path = %dest_config.display(), "config.toml already exists; leaving unchanged (use --force to overwrite)");
        println!(
            "{} already exists; left unchanged (use --force to overwrite)",
            dest_config.display()
        );
    } else {
        tracing::debug!(path = %dest_config.display(), "writing config.toml stub");
        fs::write(&dest_config, CONFIG_STUB)
            .with_context(|| format!("failed to write {}", dest_config.display()))?;
        println!("wrote {}", dest_config.display());
    }

    println!(
        "\nEdit {} to set your Actual credentials and account IDs.",
        dest_config.display()
    );
    tracing::info!(config_dir = %dest.display(), "init complete");
    Ok(ExitCode::SUCCESS)
}
