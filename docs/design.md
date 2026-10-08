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
  local config file, never committed. Order: `BAGEL_CURSEFORGE_KEY` at run time,
  then the key in settings, then `BAGEL_CURSEFORGE_KEY` at build time (so a release
  build can ship with a key that never sits in the repo).
- Modrinth and CurseForge results share one set of types (search hits, project
  details, versions), so Discover and project pages treat both sites the same.
  Content manifests record each file's `source`.
- CurseForge files whose authors block third-party downloads are never fetched
  around that choice; the UI links to the file's page instead.
- The Azure client ID is not secret and can live in the code. Microsoft login
  needs the app approved for the Minecraft API (aka.ms/mce-reviewappid).

## Roadmap
1. ~~Vanilla launch from the CLI (offline)~~ done
2. Microsoft login (built; waiting on Mojang approval of the client ID)
3. ~~Instances + Tauri UI shell~~ done
4. ~~Fabric and Quilt~~ done
5. ~~Modrinth browsing/installing + `.mrpack` import~~ done
6. ~~Forge and NeoForge~~ done
7. ~~CurseForge browsing/installing + pack import~~ done
8. Polish: settings, live log viewer, auto-update
   - ~~Install only versions that fit the instance; grey out instances and versions that won't work~~ done
