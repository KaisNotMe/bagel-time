//! `bagel`: a developer command line for the launcher core, used to test
//! downloads and launching before the desktop app exists.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, bail};
use bagel_core::account::is_valid_username;
use bagel_core::modrinth::{Modrinth, ProjectType, SearchQuery};
use bagel_core::content::{ContentKind, InstanceContent};
use bagel_core::{
    Account, Accounts, GameVersion, InstanceStore, LaunchOptions, Launcher, Loader, Paths, Progress, ProgressEvent, mrpack,
};
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
    /// Search Modrinth for mods or modpacks.
    Search {
        text: String,
        /// Search modpacks instead of mods.
        #[arg(long)]
        modpacks: bool,
        /// Only results for this Minecraft version.
        #[arg(long)]
        mc: Option<String>,
        /// Only mods for this loader.
        #[arg(long)]
        loader: Option<Loader>,
        #[arg(short = 'n', long, default_value_t = 10)]
        limit: u32,
    },
    /// List app instances.
    Instances,
    /// List the mods in an instance.
    Mods {
        instance: String,
        /// Also check Modrinth for updates.
        #[arg(long)]
        updates: bool,
    },
    /// List the worlds in an instance.
    Worlds { instance: String },
    /// Install a Modrinth mod (with its dependencies) into an instance.
    AddMod {
        instance: String,
        /// Project slug or id, e.g. sodium.
        project: String,
    },
    /// Create an instance from a .mrpack file or a Modrinth modpack slug.
    Import { source: String },
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
    let store = InstanceStore::new(&paths);
    let modrinth = Modrinth::new();
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
                extra_jvm_args: Vec::new(),
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
        Command::Search { text, modpacks, mc, loader, limit } => {
            let query = SearchQuery {
                text,
                project_type: Some(if modpacks { ProjectType::Modpack } else { ProjectType::Mod }),
                game_version: mc,
                loader,
                limit,
                ..Default::default()
            };
            let results = modrinth.search(&query).await?;
            println!("{} results", results.total_hits);
            for hit in results.hits {
                println!("{:<24} {:>10} downloads  {}", hit.slug, hit.downloads, hit.title);
            }
        }
        Command::Instances => {
            for i in store.list().await? {
                println!("{:<24} {}", i.id, describe(&i.game()));
            }
        }
        Command::Mods { instance, updates } => {
            let instance = store.get(&instance).await?;
            let mods = InstanceContent::new(&store, &instance, ContentKind::Mod)?;
            mods.identify(&modrinth).await?;
            for m in mods.list().await? {
                let state = if m.enabled { " " } else { "x" };
                let version = m.version_number.as_deref().unwrap_or("?");
                println!("{state} {:<32} {:<20} {}", m.title, version, m.file_name);
            }
            if updates {
                for u in mods.check_updates(&modrinth).await? {
                    println!("update: {} -> {}", u.file_name, u.version_number);
                }
            }
        }
        Command::Worlds { instance } => {
            let instance = store.get(&instance).await?;
            for w in bagel_core::game_files::worlds(&store.game_dir(&instance)?).await? {
                let mode = if w.hardcore { "hardcore" } else { w.game_mode.as_deref().unwrap_or("?") };
                let version = w.version.as_deref().unwrap_or("?");
                println!("{:<24} {:<10} {:<8} {}", w.name, mode, version, w.folder);
            }
        }
        Command::AddMod { instance, project } => {
            let instance = store.get(&instance).await?;
            let mods = InstanceContent::new(&store, &instance, ContentKind::Mod)?;
            let plan = mods.plan(&modrinth, &project, None).await?;
            for p in &plan {
                let kind = if p.dependency { "dependency" } else { "mod" };
                println!("{kind}: {} {}", p.version.name, p.version.version_number);
            }
            let (progress, bar) = progress_bar();
            mods.install(&modrinth, &plan, &progress).await?;
            bar.finish_and_clear();
            println!("Installed into {}", mods.dir().display());
        }
        Command::Import { source } => {
            let (progress, bar) = progress_bar();
            let path = PathBuf::from(&source);
            let instance = if path.is_file() {
                mrpack::install_pack(&store, &modrinth, &path, &progress).await?
            } else {
                mrpack::install_from_modrinth(launcher.paths(), &store, &modrinth, &source, None, &progress).await?
            };
            bar.finish_and_clear();
            println!("Created instance {} ({})", instance.id, describe(&instance.game()));
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
