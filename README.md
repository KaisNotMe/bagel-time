# Bagel Time

A Minecraft: Java Edition launcher written in Rust, in the spirit of the Modrinth App and GDLauncher.

> Early hobby project, built for personal use first. Not affiliated with Mojang or Microsoft.

## What it does

- Installs and launches any Minecraft release or snapshot, including very old versions
- Downloads the right Java runtime automatically (from Mojang's runtime manifest)
- Fabric, Quilt, Forge and NeoForge instances, each with its own game folder and pinned loader version (Forge and NeoForge are installed by running their official installers' processors)
- Microsoft account sign-in (device code flow → Xbox Live → Minecraft services), or offline mode
- Live game log, progress while downloading, start/stop from the library
- Modrinth and CurseForge: search and install mods, resource packs and shaders with their dependencies, switch them on and off, update them, and install modpacks (or import `.mrpack` and CurseForge `.zip` files)
- Recognises mods added by hand (Modrinth by SHA-1, CurseForge by fingerprint)
- Servers per instance (the same list as the game's Multiplayer screen) with live status, players and ping; Join starts the game and connects in one click; servers you played recently show on Home
- Host a server for friends in a few clicks: pick an instance (its mods come along, client-only ones are left out), agree to the EULA, and it installs, starts and gets a free playit.gg address friends can join from anywhere, with no port forwarding. Live console, players list, whitelist and settings
- Only installs versions that fit the instance: the install picker and version list grey out what won't work and say why; "New instance" from there only offers loaders and versions the project supports and installs it straight away

CurseForge needs an API key: paste one in Settings, or set `BAGEL_CURSEFORGE_KEY` (at build or run time). A key in Settings wins.

## How sign-in works

Bagel Time uses Microsoft's device code flow with the `XboxLive.signin` scope.
The player signs in on Microsoft's own page; the launcher never sees their password.
The Microsoft token is exchanged for Xbox Live and XSTS tokens, then for a Minecraft
access token from `api.minecraftservices.com`. Only the Microsoft refresh token is
stored, locally in the user's app data folder; Minecraft tokens are fetched per launch.

## Project layout

| Path | What's there |
| --- | --- |
| `crates/bagel-core` | All launcher logic: downloads, Java, launching, auth, instances, loaders |
| `crates/bagel-cli` | `bagel`, a developer command line for the core |
| `app/` | The desktop app: Tauri v2 + Svelte |
| `docs/design.md` | Design notes and roadmap |

## Building

Needs Rust (stable) and Node.js.

```sh
# desktop app (from app/)
npm install
npm run tauri dev

# developer CLI
cargo run -p bagel -- versions
cargo run -p bagel -- launch latest --loader fabric --offline Steve
```

## License

MIT
