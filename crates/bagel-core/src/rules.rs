use std::collections::HashMap;

use serde::Deserialize;

/// A Mojang rule deciding whether a library or argument applies on this machine.
#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    pub action: RuleAction,
    pub os: Option<OsRule>,
    pub features: Option<HashMap<String, bool>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleAction {
    Allow,
    Disallow,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OsRule {
    pub name: Option<String>,
    pub arch: Option<String>,
}

/// The machine the game will run on, in the vocabulary Mojang's JSON uses.
#[derive(Debug, Clone)]
pub struct Environment {
    /// "windows", "osx" or "linux".
    pub os_name: String,
    /// "x86_64", "x86", "arm64", ...
    pub arch: String,
    /// Optional launcher features such as "has_custom_resolution". Missing means off.
    pub features: HashMap<String, bool>,
}

impl Environment {
    pub fn current() -> Self {
        let os_name = match std::env::consts::OS {
            "macos" => "osx",
            other => other,
        };
        let arch = match std::env::consts::ARCH {
            "aarch64" => "arm64",
            other => other,
        };
        Self {
            os_name: os_name.to_string(),
            arch: arch.to_string(),
            features: HashMap::new(),
        }
    }

    /// Value substituted for `${arch}` in old native classifiers.
    pub fn bits(&self) -> &'static str {
        if self.arch == "x86" { "32" } else { "64" }
    }
}

impl Rule {
    fn matches(&self, env: &Environment) -> bool {
        if let Some(os) = &self.os {
            if os.name.as_deref().is_some_and(|n| n != env.os_name) {
                return false;
            }
            if os.arch.as_deref().is_some_and(|a| a != env.arch) {
                return false;
            }
        }
        if let Some(features) = &self.features {
            for (name, wanted) in features {
                if env.features.get(name).copied().unwrap_or(false) != *wanted {
                    return false;
                }
            }
        }
        true
    }
}

/// Mojang semantics: no rules means allowed; otherwise the last matching rule wins
/// and nothing matching means disallowed.
pub fn rules_allow(rules: &[Rule], env: &Environment) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for rule in rules {
        if rule.matches(env) {
            allowed = rule.action == RuleAction::Allow;
        }
    }
    allowed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(os: &str) -> Environment {
        Environment {
            os_name: os.into(),
            arch: "x86_64".into(),
            features: HashMap::new(),
        }
    }

    fn rules(json: &str) -> Vec<Rule> {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn empty_rules_allow() {
        assert!(rules_allow(&[], &env("windows")));
    }

    #[test]
    fn allow_all_except_osx() {
        let r = rules(r#"[{"action":"allow"},{"action":"disallow","os":{"name":"osx"}}]"#);
        assert!(rules_allow(&r, &env("windows")));
        assert!(!rules_allow(&r, &env("osx")));
    }

    #[test]
    fn allow_only_on_os() {
        let r = rules(r#"[{"action":"allow","os":{"name":"osx"}}]"#);
        assert!(!rules_allow(&r, &env("windows")));
        assert!(rules_allow(&r, &env("osx")));
    }

    #[test]
    fn features_default_to_off() {
        let r = rules(r#"[{"action":"allow","features":{"is_demo_user":true}}]"#);
        assert!(!rules_allow(&r, &env("windows")));
        let mut e = env("windows");
        e.features.insert("is_demo_user".into(), true);
        assert!(rules_allow(&r, &e));
    }

    #[test]
    fn arch_rule() {
        let r = rules(r#"[{"action":"allow","os":{"arch":"x86"}}]"#);
        assert!(!rules_allow(&r, &env("windows")));
    }
}
