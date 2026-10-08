//! `bagel`: a developer command line for the launcher core, used to test
//! downloads and launching before the desktop app exists.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, bail};
use bagel_core::account::is_valid_username;
use bagel_core::{Account, LaunchOptions, Launcher, Paths, Progress, ProgressEvent};
use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};

#[derive(Parser)]
#[command(name = "bagel", version, about = "Bagel Time launcher (developer CLI)")]
struct Cli {
    /// Use a different data folder instead of the default one.
    #[arg(long, global = true)]
    data_dir: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List Minecraft versions.
    Versions {
        /// Include snapshots.
        #[arg(long)]
        snapshots: bool,
        /// How many to show.
        #[arg(short = 'n', long, default_value_t = 15)]
        limit: usize,
    },
    /// Download a version without starting it.
    Install {
        /// Version id, or "latest".
        version: String,
    },
    /// Download (if needed) and start a version in offline mode.
    Launch {
        /// Version id, or "latest".
        version: String,
        /// Offline username.
        #[arg(long, default_value = "Player")]
        name: String,
        /// Maximum memory in MB.
        #[arg(long, default_value_t = 4096)]
        memory: u32,
        /// Game folder (saves, options, mods). Defaults to instances/<version>.
        #[arg(long)]
        game_dir: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let paths = match cli.data_dir {
        Some(dir) => Paths::new(dir),
        None => Paths::default_location().context("could not find a data folder for this OS")?,
    };
    let launcher = Launcher::new(paths);

    match cli.command {
        Command::Versions { snapshots, limit } => {
            let manifest = launcher.version_manifest().await?;
            println!("Latest release: {}", manifest.latest.release);
            println!("Latest snapshot: {}\n", manifest.latest.snapshot);
            for v in manifest
                .versions
                .iter()
                .filter(|v| snapshots || v.kind == "release")
                .take(limit)
            {
                println!("{:<24} {:<10} {}", v.id, v.kind, &v.release_time[..10]);
            }
        }
        Command::Install { version } => {
            let id = resolve_version(&launcher, &version).await?;
            let (progress, bar) = progress_bar();
            launcher
                .install(&id, &launcher.default_game_dir(&id), &progress)
                .await?;
            bar.finish_and_clear();
            println!("Installed {id} into {}", launcher.paths().root().display());
        }
        Command::Launch {
            version,
            name,
            memory,
            game_dir,
        } => {
            if !is_valid_username(&name) {
                bail!("'{name}' isn't a valid username (3-16 letters, digits or _)");
            }
            let id = resolve_version(&launcher, &version).await?;
            let (progress, bar) = progress_bar();
            let options = LaunchOptions {
                account: Account::offline(&name),
                game_dir,
                memory_mb: memory,
            };
            let mut cmd = launcher.prepare_launch(&id, &options, &progress).await?;
            bar.finish_and_clear();

            println!("Starting Minecraft {id} as {name}...");
            let status = cmd
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .spawn()
                .context("failed to start Java")?
                .wait()
                .await?;
            println!("Minecraft exited with {status}");
        }
    }
    Ok(())
}

async fn resolve_version(launcher: &Launcher, version: &str) -> anyhow::Result<String> {
    if version == "latest" {
        Ok(launcher.version_manifest().await?.latest.release)
    } else {
        Ok(version.to_string())
    }
}

fn progress_bar() -> (Progress, ProgressBar) {
    let bar = ProgressBar::new(0);
    bar.set_style(
        ProgressStyle::with_template("{spinner} {msg:<28} [{bar:30}] {pos}/{len}")
            .expect("valid template")
            .progress_chars("=> "),
    );
    bar.enable_steady_tick(Duration::from_millis(100));
    let b = bar.clone();
    let progress = Progress::new(move |event| match event {
        ProgressEvent::Stage { name, total } => {
            b.set_message(name);
            b.set_length(total);
            b.set_position(0);
        }
        ProgressEvent::Advance(n) => b.inc(n),
    });
    (progress, bar)
}
