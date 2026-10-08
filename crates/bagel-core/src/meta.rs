//! Mojang's version manifest and per-version JSON.

use std::collections::HashMap;

use serde::Deserialize;

use crate::rules::Rule;

pub const VERSION_MANIFEST_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

#[derive(Debug, Clone, Deserialize)]
pub struct VersionManifest {
    pub latest: Latest,
    pub versions: Vec<ManifestEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Latest {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestEntry {
    pub id: String,
    /// "release", "snapshot", "old_beta" or "old_alpha".
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    pub sha1: String,
    pub release_time: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionJson {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub main_class: String,
    /// 1.13+ structured arguments.
    pub arguments: Option<Arguments>,
    /// Pre-1.13 space-separated game arguments.
    pub minecraft_arguments: Option<String>,
    pub asset_index: AssetIndexRef,
    pub assets: String,
    #[serde(default)]
    pub downloads: HashMap<String, Download>,
    pub java_version: Option<JavaVersion>,
    #[serde(default)]
    pub libraries: Vec<Library>,
    pub logging: Option<Logging>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<Argument>,
    #[serde(default)]
    pub jvm: Vec<Argument>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Argument {
    Plain(String),
    Conditional { rules: Vec<Rule>, value: ArgValue },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ArgValue {
    One(String),
    Many(Vec<String>),
}

impl ArgValue {
    pub fn values(&self) -> &[String] {
        match self {
            ArgValue::One(v) => std::slice::from_ref(v),
            ArgValue::Many(v) => v,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetIndexRef {
    pub id: String,
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Download {
    pub url: String,
    pub sha1: Option<String>,
    pub size: Option<u64>,
    /// Only set for library artifacts: path relative to the libraries folder.
    pub path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaVersion {
    pub component: String,
    pub major_version: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Library {
    /// Maven coordinate, e.g. `org.lwjgl:lwjgl:3.3.3`.
    pub name: String,
    pub downloads: Option<LibraryDownloads>,
    #[serde(default)]
    pub rules: Vec<Rule>,
    /// Old-style natives: OS name -> classifier (may contain `${arch}`).
    pub natives: Option<HashMap<String, String>>,
    pub extract: Option<Extract>,
    /// Maven repository base, used by mod loader libraries that have no `downloads`.
    pub url: Option<String>,
    /// Mod loader metadata puts hashes on the library itself instead of `downloads`.
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

/// A mod loader's version JSON (Fabric, Quilt): extra libraries and arguments
/// layered on top of the vanilla version named by `inherits_from`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoaderProfile {
    pub id: String,
    pub inherits_from: String,
    pub main_class: String,
    pub arguments: Option<Arguments>,
    pub minecraft_arguments: Option<String>,
    #[serde(default)]
    pub libraries: Vec<Library>,
}

impl VersionJson {
    /// Apply a loader profile the way Mojang's launcher handles `inheritsFrom`:
    /// the profile's libraries come first and replace vanilla copies of the same
    /// library, arguments are appended, and the main class is swapped.
    pub fn with_profile(mut self, profile: LoaderProfile) -> VersionJson {
        let overridden: std::collections::HashSet<String> =
            profile.libraries.iter().map(|l| library_key(&l.name)).collect();
        let mut libraries = profile.libraries;
        libraries.extend(
            self.libraries
                .into_iter()
                .filter(|l| !overridden.contains(&library_key(&l.name))),
        );
        self.libraries = libraries;
        self.id = profile.id;
        self.main_class = profile.main_class;
        if let Some(extra) = profile.arguments {
            let args = self.arguments.get_or_insert(Arguments {
                game: Vec::new(),
                jvm: Vec::new(),
            });
            args.game.extend(extra.game);
            args.jvm.extend(extra.jvm);
        }
        if profile.minecraft_arguments.is_some() {
            self.minecraft_arguments = profile.minecraft_arguments;
        }
        self
    }
}

/// `group:artifact[:classifier]`: a library's identity without its version.
fn library_key(coord: &str) -> String {
    let coord = coord.split_once('@').map_or(coord, |(c, _)| c);
    let parts: Vec<&str> = coord.split(':').collect();
    match parts.as_slice() {
        [group, artifact, _version] => format!("{group}:{artifact}"),
        [group, artifact, _version, classifier] => format!("{group}:{artifact}:{classifier}"),
        _ => coord.to_string(),
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct LibraryDownloads {
    pub artifact: Option<Download>,
    #[serde(default)]
    pub classifiers: HashMap<String, Download>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Extract {
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Logging {
    pub client: Option<LoggingConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoggingConfig {
    /// e.g. `-Dlog4j.configurationFile=${path}`.
    pub argument: String,
    pub file: LoggingFile,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoggingFile {
    pub id: String,
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

/// Relative path of a Maven coordinate: `group:artifact:version[:classifier][@ext]`.
pub fn maven_path(coord: &str) -> Option<String> {
    let (coord, ext) = coord.split_once('@').unwrap_or((coord, "jar"));
    let mut parts = coord.split(':');
    let group = parts.next()?;
    let artifact = parts.next()?;
    let version = parts.next()?;
    let classifier = parts.next();
    let file = match classifier {
        Some(c) => format!("{artifact}-{version}-{c}.{ext}"),
        None => format!("{artifact}-{version}.{ext}"),
    };
    Some(format!(
        "{}/{artifact}/{version}/{file}",
        group.replace('.', "/")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maven_path_basic() {
        assert_eq!(
            maven_path("org.lwjgl:lwjgl:3.3.3").unwrap(),
            "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3.jar"
        );
    }

    #[test]
    fn maven_path_classifier_and_ext() {
        assert_eq!(
            maven_path("net.fabricmc:intermediary:1.21:v2@zip").unwrap(),
            "net/fabricmc/intermediary/1.21/intermediary-1.21-v2.zip"
        );
    }

    #[test]
    fn maven_path_rejects_garbage() {
        assert!(maven_path("nope").is_none());
    }

    #[test]
    fn library_keys_ignore_version() {
        assert_eq!(library_key("org.ow2.asm:asm:9.7"), "org.ow2.asm:asm");
        assert_eq!(
            library_key("org.lwjgl:lwjgl:3.3.3:natives-windows"),
            "org.lwjgl:lwjgl:natives-windows"
        );
    }

    #[test]
    fn profile_merges_onto_vanilla() {
        let vanilla: VersionJson = serde_json::from_str(
            r#"{"id":"1.21","type":"release","mainClass":"net.minecraft.client.main.Main",
                "assets":"17","assetIndex":{"id":"17","sha1":"x","size":1,"url":"u"},
                "arguments":{"game":["--username","${auth_player_name}"],"jvm":["-cp","${classpath}"]},
                "libraries":[
                  {"name":"org.ow2.asm:asm:9.3","downloads":{"artifact":{"url":"v","path":"a.jar"}}},
                  {"name":"org.lwjgl:lwjgl:3.3.3"},
                  {"name":"org.lwjgl:lwjgl:3.3.3:natives-windows"}
                ]}"#,
        )
        .unwrap();
        let profile: LoaderProfile = serde_json::from_str(
            r#"{"id":"fabric-loader-0.16.10-1.21","inheritsFrom":"1.21",
                "mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient",
                "arguments":{"game":[],"jvm":["-DFabricMcEmu= net.minecraft.client.main.Main "]},
                "libraries":[
                  {"name":"org.ow2.asm:asm:9.7","url":"https://maven.fabricmc.net/","sha1":"abc","size":5},
                  {"name":"net.fabricmc:fabric-loader:0.16.10","url":"https://maven.fabricmc.net/"}
                ]}"#,
        )
        .unwrap();

        let merged = vanilla.with_profile(profile);
        let names: Vec<_> = merged.libraries.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "org.ow2.asm:asm:9.7",
                "net.fabricmc:fabric-loader:0.16.10",
                "org.lwjgl:lwjgl:3.3.3",
                "org.lwjgl:lwjgl:3.3.3:natives-windows",
            ]
        );
        assert_eq!(merged.id, "fabric-loader-0.16.10-1.21");
        assert_eq!(merged.main_class, "net.fabricmc.loader.impl.launch.knot.KnotClient");
        let args = merged.arguments.unwrap();
        assert_eq!(args.jvm.len(), 3);
        assert_eq!(args.game.len(), 2);
        assert_eq!(merged.libraries[0].sha1.as_deref(), Some("abc"));
        // Vanilla-only fields survive.
        assert_eq!(merged.assets, "17");
    }

    #[test]
    fn parses_conditional_arguments() {
        let args: Arguments = serde_json::from_str(
            r#"{"game":["--username","${auth_player_name}",
                {"rules":[{"action":"allow","features":{"is_demo_user":true}}],"value":"--demo"}],
                "jvm":[{"rules":[{"action":"allow","os":{"name":"osx"}}],"value":["-XstartOnFirstThread"]}]}"#,
        )
        .unwrap();
        assert_eq!(args.game.len(), 3);
        assert!(matches!(&args.jvm[0], Argument::Conditional { value: ArgValue::Many(v), .. } if v.len() == 1));
    }
}
