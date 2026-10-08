//! Building the java command line from a version JSON.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::account::Account;
use crate::meta::{Argument, VersionJson};
use crate::rules::{Environment, rules_allow};

pub struct LaunchContext<'a> {
    pub version: &'a VersionJson,
    pub account: &'a Account,
    pub env: &'a Environment,
    pub game_dir: &'a Path,
    pub assets_root: &'a Path,
    /// Differs from `assets_root` only for legacy versions.
    pub game_assets: &'a Path,
    pub libraries_dir: &'a Path,
    pub natives_dir: &'a Path,
    pub classpath: &'a [PathBuf],
    pub log_config: Option<&'a Path>,
    pub memory_mb: u32,
    /// Added after the memory setting, so they can override it.
    pub extra_jvm_args: &'a [String],
    /// Join this server (`host` or `host:port`) as soon as the game starts.
    pub server: Option<&'a str>,
}

/// Every argument after the java executable, with `${...}` placeholders filled in.
pub fn build_arguments(ctx: &LaunchContext) -> Vec<String> {
    let separator = if ctx.env.os_name == "windows" { ";" } else { ":" };
    let classpath = ctx
        .classpath
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(separator);
    let uuid = ctx.account.uuid.simple().to_string();
    let path = |p: &Path| p.display().to_string();

    let mut vars: HashMap<&str, String> = HashMap::from([
        ("auth_player_name", ctx.account.username.clone()),
        ("auth_uuid", uuid.clone()),
        ("auth_access_token", ctx.account.access_token.clone()),
        ("auth_session", format!("token:{}:{uuid}", ctx.account.access_token)),
        ("auth_xuid", ctx.account.xuid().to_string()),
        ("user_type", ctx.account.user_type().to_string()),
        ("user_properties", "{}".to_string()),
        ("clientid", String::new()),
        ("version_name", ctx.version.id.clone()),
        ("version_type", ctx.version.kind.clone()),
        ("game_directory", path(ctx.game_dir)),
        ("assets_root", path(ctx.assets_root)),
        ("game_assets", path(ctx.game_assets)),
        ("assets_index_name", ctx.version.assets.clone()),
        ("natives_directory", path(ctx.natives_dir)),
        ("library_directory", path(ctx.libraries_dir)),
        ("classpath_separator", separator.to_string()),
        ("classpath", classpath),
        ("launcher_name", "bagel-time".to_string()),
        ("launcher_version", env!("CARGO_PKG_VERSION").to_string()),
    ]);

    // 1.20+ joins through Quick Play; older versions take --server/--port.
    let quick_play = ctx.server.is_some() && supports_quick_play(ctx.version);
    let quick_env;
    let env = if quick_play {
        let mut e = ctx.env.clone();
        e.features.insert("is_quick_play_multiplayer".into(), true);
        quick_env = e;
        &quick_env
    } else {
        ctx.env
    };
    if let Some(server) = ctx.server {
        vars.insert("quickPlayMultiplayer", server.to_string());
    }

    let mut args = vec![format!("-Xmx{}M", ctx.memory_mb)];
    args.extend(ctx.extra_jvm_args.iter().cloned());
    if let (Some(config), Some(file)) = (
        ctx.version.logging.as_ref().and_then(|l| l.client.as_ref()),
        ctx.log_config,
    ) {
        args.push(config.argument.replace("${path}", &path(file)));
    }

    // Pre-1.13 versions use a plain argument string and imply the JVM
    // arguments. A loader profile may still add structured JVM arguments.
    let legacy = ctx
        .version
        .minecraft_arguments
        .as_deref()
        .filter(|_| ctx.version.arguments.as_ref().is_none_or(|a| a.game.is_empty()));

    if legacy.is_some() {
        if ctx.env.os_name == "osx" {
            args.push("-XstartOnFirstThread".into());
        }
        args.extend(
            ["-Djava.library.path=${natives_directory}", "-cp", "${classpath}"].map(String::from),
        );
    }
    if let Some(a) = &ctx.version.arguments {
        push_arguments(&mut args, &a.jvm, env);
    }

    args.push(ctx.version.main_class.clone());

    match (legacy, &ctx.version.arguments) {
        (Some(legacy), _) => args.extend(legacy.split_whitespace().map(String::from)),
        (None, Some(a)) => push_arguments(&mut args, &a.game, env),
        (None, None) => {}
    }
    if let (Some(server), false) = (ctx.server, quick_play) {
        let (host, port) = crate::servers::parse_address(server).unwrap_or((server.to_string(), None));
        args.extend(["--server".into(), host, "--port".into(), port.unwrap_or(25565).to_string()]);
    }

    args.iter().map(|a| substitute(a, &vars)).collect()
}

/// Whether the version can join a server with `--quickPlayMultiplayer`.
pub fn supports_quick_play(version: &VersionJson) -> bool {
    version.arguments.as_ref().is_some_and(|a| {
        a.game.iter().any(|arg| {
            matches!(arg, Argument::Conditional { rules, .. }
                if rules.iter().any(|r| r.features.as_ref().is_some_and(|f| f.contains_key("is_quick_play_multiplayer"))))
        })
    })
}

fn push_arguments(out: &mut Vec<String>, args: &[Argument], env: &Environment) {
    for arg in args {
        match arg {
            Argument::Plain(s) => out.push(s.clone()),
            Argument::Conditional { rules, value } => {
                if rules_allow(rules, env) {
                    out.extend(value.values().iter().cloned());
                }
            }
        }
    }
}

/// Replace `${name}` with its value. Unknown placeholders are left untouched.
fn substitute(arg: &str, vars: &HashMap<&str, String>) -> String {
    let mut out = String::with_capacity(arg.len());
    let mut rest = arg;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find('}') {
            Some(end) => {
                let key = &after[..end];
                match vars.get(key) {
                    Some(v) => out.push_str(v),
                    None => out.push_str(&rest[start..start + 2 + end + 1]),
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(json: &str) -> VersionJson {
        serde_json::from_str(json).unwrap()
    }

    const BASE: &str = r#""id":"1.21","type":"release","mainClass":"net.minecraft.client.main.Main",
        "assets":"17","assetIndex":{"id":"17","sha1":"x","size":1,"url":"u"}"#;

    fn run(v: &VersionJson) -> Vec<String> {
        run_with(v, &[])
    }

    fn run_with(v: &VersionJson, extra: &[String]) -> Vec<String> {
        run_full(v, extra, None)
    }

    fn run_full(v: &VersionJson, extra: &[String], server: Option<&str>) -> Vec<String> {
        let env = Environment {
            os_name: "windows".into(),
            arch: "x86_64".into(),
            features: Default::default(),
        };
        let account = Account::offline("Steve");
        let cp = [PathBuf::from("a.jar"), PathBuf::from("b.jar")];
        build_arguments(&LaunchContext {
            version: v,
            account: &account,
            env: &env,
            game_dir: Path::new("G"),
            assets_root: Path::new("A"),
            game_assets: Path::new("A"),
            libraries_dir: Path::new("L"),
            natives_dir: Path::new("N"),
            classpath: &cp,
            log_config: None,
            memory_mb: 2048,
            extra_jvm_args: extra,
            server,
        })
    }

    #[test]
    fn substitute_handles_known_unknown_and_broken() {
        let vars = HashMap::from([("a", "1".to_string())]);
        assert_eq!(substitute("x=${a}!", &vars), "x=1!");
        assert_eq!(substitute("${nope}", &vars), "${nope}");
        assert_eq!(substitute("${a", &vars), "${a");
    }

    #[test]
    fn modern_arguments() {
        let v = version(&format!(
            r#"{{{BASE},"arguments":{{
                "jvm":["-Djava.library.path=${{natives_directory}}","-cp","${{classpath}}",
                       {{"rules":[{{"action":"allow","os":{{"name":"osx"}}}}],"value":"-XstartOnFirstThread"}}],
                "game":["--username","${{auth_player_name}}","--assetIndex","${{assets_index_name}}",
                        {{"rules":[{{"action":"allow","features":{{"is_demo_user":true}}}}],"value":"--demo"}}]}}}}"#
        ));
        let args = run(&v);
        assert_eq!(
            args,
            vec![
                "-Xmx2048M",
                "-Djava.library.path=N",
                "-cp",
                "a.jar;b.jar",
                "net.minecraft.client.main.Main",
                "--username",
                "Steve",
                "--assetIndex",
                "17",
            ]
        );
    }

    #[test]
    fn extra_jvm_arguments_follow_memory() {
        let v = version(&format!(r#"{{{BASE},"arguments":{{"jvm":[],"game":[]}}}}"#));
        let args = run_with(&v, &["-Xmx8G".into(), "-XX:+UseZGC".into()]);
        assert_eq!(&args[..3], ["-Xmx2048M", "-Xmx8G", "-XX:+UseZGC"]);
    }

    #[test]
    fn legacy_arguments() {
        let v = version(&format!(
            r#"{{{BASE},"minecraftArguments":"--username ${{auth_player_name}} --session ${{auth_session}}"}}"#
        ));
        let args = run(&v);
        let uuid = Account::offline("Steve").uuid.simple().to_string();
        assert_eq!(args[1..4], ["-Djava.library.path=N", "-cp", "a.jar;b.jar"]);
        assert_eq!(args[4], "net.minecraft.client.main.Main");
        assert_eq!(args[5..], ["--username", "Steve", "--session", &format!("token:0:{uuid}")]);
    }

    #[test]
    fn legacy_with_extra_loader_jvm_arguments() {
        let v = version(&format!(
            r#"{{{BASE},"minecraftArguments":"--username ${{auth_player_name}}",
                "arguments":{{"jvm":["-Dloader=yes"]}}}}"#
        ));
        let args = run(&v);
        assert_eq!(
            args,
            [
                "-Xmx2048M",
                "-Djava.library.path=N",
                "-cp",
                "a.jar;b.jar",
                "-Dloader=yes",
                "net.minecraft.client.main.Main",
                "--username",
                "Steve",
            ]
        );
    }

    #[test]
    fn joins_servers_with_quick_play_or_server_flags() {
        let modern = version(&format!(
            r#"{{{BASE},"arguments":{{"jvm":[],"game":["--username","${{auth_player_name}}",
                {{"rules":[{{"action":"allow","features":{{"is_quick_play_multiplayer":true}}}}],
                  "value":["--quickPlayMultiplayer","${{quickPlayMultiplayer}}"]}}]}}}}"#
        ));
        assert!(supports_quick_play(&modern));
        let args = run_full(&modern, &[], Some("play.example.net"));
        assert_eq!(args[args.len() - 2..], ["--quickPlayMultiplayer", "play.example.net"]);
        assert!(!run(&modern).contains(&"--quickPlayMultiplayer".to_string()));

        let old = version(&format!(r#"{{{BASE},"minecraftArguments":"--username ${{auth_player_name}}"}}"#));
        assert!(!supports_quick_play(&old));
        let args = run_full(&old, &[], Some("play.example.net:25570"));
        assert_eq!(args[args.len() - 4..], ["--server", "play.example.net", "--port", "25570"]);
    }
}
