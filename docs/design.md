# Bagel Time: design notes

A Minecraft launcher in Rust, in the spirit of the Modrinth App and GDLauncher.

## Goals
- Daily-driver launcher first; possibly released publicly later (so: no hardcoded
  paths, cross-platform code, Windows tested first).
- Vanilla, Fabric, Quilt, Forge and NeoForge.
- Mods and modpacks from Modrinth (main) and CurseForge.

## Structure
- `crates/bagel-core`: all launcher logic, no UI. Downloads, Java runtimes,
  launching, (later) auth, instances, loaders, content platforms.
- `crates/bagel-cli`: `bagel` developer CLI for exercising the core.
- `app/` (later): Tauri v2 desktop app with a Svelte frontend, calling `bagel-core`.

## Data folder
`%APPDATA%\BagelTime\data` (platform equivalent elsewhere):
`versions/`, `libraries/`, `assets/` and `java/` are shared by every instance;
`instances/<name>/` holds each game folder (saves, mods, configs).

## Decisions
- Java comes from Mojang's runtime manifest, picked by the version's
  `javaVersion.component`, so players never install Java themselves.
- Downloads go to `<file>.part` and are renamed after SHA-1 verification; a file
  whose size matches is treated as installed, so re-checking before every launch is fast.
- Secrets (CurseForge API key) are read from the environment or an untracked
  local config file, never committed.
- The Azure client ID is not secret and can live in the code. Microsoft login
  needs the app approved for the Minecraft API (aka.ms/mce-reviewappid).

## Roadmap
1. ~~Vanilla launch from the CLI (offline)~~ done
2. Microsoft login
3. ~~Instances + Tauri UI shell~~ done
4. Fabric and Quilt
5. Modrinth browsing/installing + `.mrpack` import
6. Forge and NeoForge
7. CurseForge browsing + pack import
8. Polish: settings, live log viewer, auto-update
