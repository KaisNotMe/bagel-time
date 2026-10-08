//! `bagel`: a developer command line for the launcher core, used to test
//! downloads and launching before the desktop app exists.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, bail};
use bagel_core::account::is_valid_username;
use bagel_core::{Account, Accounts, GameVersion, LaunchOptions, Launcher, Loader, Paths, Progress, ProgressEvent};
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
    /// List Fabric or Quilt versions for a Minecraft version.
    Loaders {
        /// fabric or quilt.
        loader: Loader,
        /// Minecraft version id, or "latest".
        version: String,
        /// How many to show.
        #[arg(short = 'n', long, default_value_t = 10)]
        limit: usize,
    },
    /// Download a version without starting it.
    Install {
        /// Version id, or "latest".
        version: String,
        #[command(flatten)]
        loader: LoaderArgs,
    },
    /// Download (if needed) and start a version in offline mode.
    Launch {
        /// Version id, or "latest".
        version: String,
        #[command(flatten)]
        loader: LoaderArgs,
        /// Play offline with this username instead of the signed-in account.
        #[arg(long)]
        offline: Option<String>,
        /// Maximum memory in MB.
        #[arg(long, default_value_t = 4096)]
        memory: u32,
        /// Game folder (saves, options, mods). Defaults to cli-games/<version>.
        #[arg(long)]
        game_dir: Option<PathBuf>,
    },
    /// Sign in with a Microsoft account.
    Login,
    /// List signed-in accounts.
    Accounts,
    /// Remove a signed-in account.
    Logout {
        /// Minecraft username.
        username: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let paths = match cli.data_dir {
        Some(dir) => Paths::new(dir),
        None => Paths::default_location().context("could not find a data folder for this OS")?,
    };
    let accounts = Accounts::new(&paths);
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
        Command::Loaders { loader, version, limit } => {
            let id = resolve_version(&launcher, &version).await?;
            let versions = launcher.loader_versions(loader, &id).await?;
            if versions.is_empty() {
                println!("{loader} doesn't support Minecraft {id}.");
            }
            for v in versions.iter().take(limit) {
                println!("{:<16} {}", v.version, if v.stable { "stable" } else { "beta" });
            }
        }
        Command::Install { version, loader } => {
            let game = resolve_game(&launcher, &version, &loader).await?;
            let (progress, bar) = progress_bar();
            launcher
                .install(&game, &cli_game_dir(&launcher, &game), &progress)
                .await?;
            bar.finish_and_clear();
            println!("Installed {} into {}", describe(&game), launcher.paths().root().display());
        }
        Command::Launch {
            version,
            loader,
            offline,
            memory,
            game_dir,
        } => {
            let account = match (offline, accounts.active().await?) {
                (Some(name), _) => {
                    if !is_valid_username(&name) {
                        bail!("'{name}' isn't a valid username (3-16 letters, digits or _)");
                    }
                    Account::offline(&name)
                }
                (None, Some(uuid)) => {
                    println!("Signing in...");
                    accounts.launch_account(uuid).await?
                }
                (None, None) => bail!("No account signed in. Run `bagel login`, or pass --offline <name>."),
            };
            let name = account.username.clone();
            let game = resolve_game(&launcher, &version, &loader).await?;
            let (progress, bar) = progress_bar();
            let options = LaunchOptions {
                account,
                game_dir: game_dir.unwrap_or_else(|| cli_game_dir(&launcher, &game)),
                memory_mb: memory,
            };
            let mut cmd = launcher.prepare_launch(&game, &options, &progress).await?;
            bar.finish_and_clear();

            println!("Starting {} as {name}...", describe(&game));
            let status = cmd
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .spawn()
                .context("failed to start Java")?
                .wait()
                .await?;
            println!("Minecraft exited with {status}");
        }
        Command::Login => {
            let login = accounts.start_login().await?;
            println!("Open {} and enter the code: {}", login.verification_uri, login.user_code);
            println!("Waiting for you to sign in (Ctrl+C to cancel)...");
            let account = accounts.finish_login(&login).await?;
            println!("Signed in as {}.", account.username);
        }
        Command::Accounts => {
            let list = accounts.list().await?;
            if list.accounts.is_empty() {
                println!("No accounts. Run `bagel login`.");
            }
            for a in &list.accounts {
                let marker = if list.active == Some(a.uuid) { "*" } else { " " };
                println!("{marker} {:<16} {}", a.username, a.uuid);
            }
        }
        Command::Logout { username } => {
            let list = accounts.list().await?;
            let account = list
                .accounts
                .iter()
                .find(|a| a.username.eq_ignore_ascii_case(&username))
                .with_context(|| format!("no signed-in account named {username}"))?;
            accounts.remove(account.uuid).await?;
            println!("Removed {}.", account.username);
        }
    }
    Ok(())
}

#[derive(clap::Args)]
struct LoaderArgs {
    /// vanilla, fabric or quilt.
    #[arg(long, default_value = "vanilla")]
    loader: Loader,
    /// Loader version. Defaults to the newest stable one.
    #[arg(long)]
    loader_version: Option<String>,
}

async fn resolve_game(launcher: &Launcher, version: &str, args: &LoaderArgs) -> anyhow::Result<GameVersion> {
    let minecraft = resolve_version(launcher, version).await?;
    let loader_version = match (args.loader, &args.loader_version) {
        (Loader::Vanilla, _) => None,
        (_, Some(v)) => Some(v.clone()),
        (loader, None) => Some(
            launcher
                .latest_stable_loader(loader, &minecraft)
                .await?
                .with_context(|| format!("{loader} doesn't support Minecraft {minecraft}"))?,
        ),
    };
    Ok(GameVersion {
        minecraft,
        loader: args.loader,
        loader_version,
    })
}

fn describe(game: &GameVersion) -> String {
    match &game.loader_version {
        Some(v) => format!("Minecraft {} with {} {v}", game.minecraft, game.loader),
        None => format!("Minecraft {}", game.minecraft),
    }
}

/// The CLI keeps its game folders apart from the app's instances.
fn cli_game_dir(launcher: &Launcher, game: &GameVersion) -> PathBuf {
    let name = match game.loader {
        Loader::Vanilla => game.minecraft.clone(),
        loader => format!("{}-{}", loader.slug(), game.minecraft),
    };
    launcher.paths().root().join("cli-games").join(name)
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
