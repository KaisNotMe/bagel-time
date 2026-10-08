//! Modrinth API v2: searching projects, picking versions that fit an
//! instance, and resolving required dependencies.
//!
//! Docs: https://docs.modrinth.com/api/

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::download::Downloader;
use crate::error::{Error, Result};
use crate::loaders::Loader;

const API: &str = "https://api.modrinth.com/v2";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectType {
    Mod,
    Modpack,
    ResourcePack,
    Shader,
}

impl ProjectType {
    pub fn slug(self) -> &'static str {
        match self {
            ProjectType::Mod => "mod",
            ProjectType::Modpack => "modpack",
            ProjectType::ResourcePack => "resourcepack",
            ProjectType::Shader => "shader",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortBy {
    #[default]
    Relevance,
    Downloads,
    Follows,
    Newest,
    Updated,
}

impl SortBy {
    fn slug(self) -> &'static str {
        match self {
            SortBy::Relevance => "relevance",
            SortBy::Downloads => "downloads",
            SortBy::Follows => "follows",
            SortBy::Newest => "newest",
            SortBy::Updated => "updated",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SearchQuery {
    pub text: String,
    pub project_type: Option<ProjectType>,
    /// Only projects with a version for this Minecraft version.
    pub game_version: Option<String>,
    /// Only projects that run on this loader (mods and modpacks).
    pub loader: Option<Loader>,
    /// Only projects in all of these categories.
    pub categories: Vec<String>,
    pub sort: SortBy,
    pub offset: u32,
    pub limit: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct SearchHit {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub author: String,
    #[serde(default)]
    pub icon_url: Option<String>,
    pub downloads: u64,
    #[serde(default)]
    pub follows: u64,
    #[serde(default)]
    pub display_categories: Vec<String>,
    pub project_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct SearchResults {
    pub hits: Vec<SearchHit>,
    pub offset: u32,
    pub limit: u32,
    pub total_hits: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct Project {
    pub id: String,
    pub slug: String,
    pub title: String,
    #[serde(default)]
    pub icon_url: Option<String>,
    pub project_type: String,
}

/// Everything a project page shows.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct ProjectDetails {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    /// Markdown.
    #[serde(default)]
    pub body: String,
    pub project_type: String,
    #[serde(default)]
    pub icon_url: Option<String>,
    pub downloads: u64,
    #[serde(default)]
    pub followers: u64,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub additional_categories: Vec<String>,
    #[serde(default)]
    pub game_versions: Vec<String>,
    #[serde(default)]
    pub loaders: Vec<String>,
    pub published: String,
    pub updated: String,
    #[serde(default)]
    pub license: Option<License>,
    #[serde(default)]
    pub client_side: Option<String>,
    #[serde(default)]
    pub server_side: Option<String>,
    #[serde(default)]
    pub source_url: Option<String>,
    #[serde(default)]
    pub issues_url: Option<String>,
    #[serde(default)]
    pub wiki_url: Option<String>,
    #[serde(default)]
    pub discord_url: Option<String>,
    #[serde(default)]
    pub gallery: Vec<GalleryImage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct License {
    pub id: String,
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct GalleryImage {
    pub url: String,
    #[serde(default)]
    pub featured: bool,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub ordering: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamMember {
    pub username: String,
    pub avatar_url: Option<String>,
    pub role: String,
}

#[derive(Deserialize)]
struct ApiMember {
    user: ApiUser,
    role: String,
}

#[derive(Deserialize)]
struct ApiUser {
    username: String,
    #[serde(default)]
    avatar_url: Option<String>,
}

/// A search filter category, e.g. "optimization" for mods.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct Category {
    /// Inline SVG markup from Modrinth.
    pub icon: String,
    pub name: String,
    pub project_type: String,
    /// Group heading, e.g. "categories", "features", "resolutions".
    pub header: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct Version {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub version_number: String,
    /// release, beta or alpha.
    pub version_type: String,
    #[serde(default)]
    pub game_versions: Vec<String>,
    #[serde(default)]
    pub loaders: Vec<String>,
    pub date_published: String,
    #[serde(default)]
    pub downloads: u64,
    #[serde(default)]
    pub changelog: Option<String>,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
    pub files: Vec<VersionFile>,
}

impl Version {
    /// The file to install: the one marked primary, else the first.
    pub fn primary_file(&self) -> Option<&VersionFile> {
        self.files.iter().find(|f| f.primary).or(self.files.first())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct Dependency {
    #[serde(default)]
    pub version_id: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    /// required, optional, incompatible or embedded.
    pub dependency_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct VersionFile {
    pub hashes: FileHashes,
    pub url: String,
    pub filename: String,
    #[serde(default)]
    pub primary: bool,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileHashes {
    pub sha1: String,
    #[serde(default)]
    pub sha512: Option<String>,
}

/// Loader names Modrinth uses for mods that run on `loader`. Quilt loads
/// Fabric mods too.
pub fn mod_loaders(loader: Loader) -> &'static [&'static str] {
    match loader {
        Loader::Vanilla => &[],
        Loader::Fabric => &["fabric"],
        Loader::Quilt => &["quilt", "fabric"],
        Loader::Forge => &["forge"],
        Loader::NeoForge => &["neoforge"],
    }
}

/// Newest release if there is one, else the newest version of any kind.
/// Modrinth lists versions newest first.
pub fn pick_best(versions: &[Version]) -> Option<&Version> {
    versions
        .iter()
        .find(|v| v.version_type == "release")
        .or(versions.first())
}

#[derive(Clone, Default)]
pub struct Modrinth {
    dl: Downloader,
}

impl Modrinth {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn downloader(&self) -> &Downloader {
        &self.dl
    }

    fn url(segments: &[&str], query: &[(&str, String)]) -> String {
        let mut url = reqwest::Url::parse(API).expect("API base is a valid URL");
        url.path_segments_mut()
            .expect("API base can have a path")
            .extend(segments);
        if !query.is_empty() {
            url.query_pairs_mut().extend_pairs(query);
        }
        url.into()
    }

    pub async fn search(&self, q: &SearchQuery) -> Result<SearchResults> {
        let url = Self::url(&["search"], &search_params(q));
        self.dl.get_json(&url).await
    }

    pub async fn project(&self, id_or_slug: &str) -> Result<Project> {
        self.dl.get_json(&Self::url(&["project", id_or_slug], &[])).await
    }

    /// Several projects at once; unknown ids are left out.
    pub async fn projects(&self, ids: &[String]) -> Result<Vec<Project>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids = serde_json::to_string(ids).expect("strings serialize");
        self.dl.get_json(&Self::url(&["projects"], &[("ids", ids)])).await
    }

    pub async fn project_details(&self, id_or_slug: &str) -> Result<ProjectDetails> {
        self.dl.get_json(&Self::url(&["project", id_or_slug], &[])).await
    }

    /// The project's team, owner first.
    pub async fn members(&self, id_or_slug: &str) -> Result<Vec<TeamMember>> {
        let members: Vec<ApiMember> = self
            .dl
            .get_json(&Self::url(&["project", id_or_slug, "members"], &[]))
            .await?;
        let mut out: Vec<TeamMember> = members
            .into_iter()
            .map(|m| TeamMember {
                username: m.user.username,
                avatar_url: m.user.avatar_url,
                role: m.role,
            })
            .collect();
        out.sort_by_key(|m| m.role != "Owner");
        Ok(out)
    }

    pub async fn categories(&self) -> Result<Vec<Category>> {
        self.dl.get_json(&Self::url(&["tag", "category"], &[])).await
    }

    pub async fn version(&self, id: &str) -> Result<Version> {
        self.dl.get_json(&Self::url(&["version", id], &[])).await
    }

    /// A project's versions, newest first, optionally limited to loaders and
    /// game versions.
    pub async fn project_versions(
        &self,
        project: &str,
        loaders: &[&str],
        game_versions: &[&str],
    ) -> Result<Vec<Version>> {
        let mut query = Vec::new();
        if !loaders.is_empty() {
            query.push(("loaders", serde_json::to_string(loaders).expect("strings serialize")));
        }
        if !game_versions.is_empty() {
            query.push((
                "game_versions",
                serde_json::to_string(game_versions).expect("strings serialize"),
            ));
        }
        self.dl
            .get_json(&Self::url(&["project", project, "version"], &query))
            .await
    }

    /// Versions that fit a target (Minecraft version and loaders), newest first.
    pub async fn compatible_versions(&self, project: &str, target: &Target) -> Result<Vec<Version>> {
        self.project_versions(project, &target.loaders, &[&target.minecraft])
            .await
    }

    /// Looks up files by SHA-1. Returns hash -> version for the ones Modrinth knows.
    pub async fn versions_by_sha1(&self, hashes: &[String]) -> Result<HashMap<String, Version>> {
        if hashes.is_empty() {
            return Ok(HashMap::new());
        }
        #[derive(Serialize)]
        struct Body<'a> {
            hashes: &'a [String],
            algorithm: &'a str,
        }
        let body = Body { hashes, algorithm: "sha1" };
        self.dl.post_json(&Self::url(&["version_files"], &[]), &body).await
    }

    /// Newest compatible version for each file, by SHA-1. Files that are
    /// already up to date map to their own version.
    pub async fn latest_by_sha1(&self, hashes: &[String], target: &Target) -> Result<HashMap<String, Version>> {
        if hashes.is_empty() {
            return Ok(HashMap::new());
        }
        #[derive(Serialize)]
        struct Body<'a> {
            hashes: &'a [String],
            algorithm: &'a str,
            loaders: &'a [&'a str],
            game_versions: [&'a str; 1],
        }
        let body = Body {
            hashes,
            algorithm: "sha1",
            loaders: &target.loaders,
            game_versions: [&target.minecraft],
        };
        self.dl
            .post_json(&Self::url(&["version_files", "update"], &[]), &body)
            .await
    }

    /// Works out what to download to install a project into an instance:
    /// the chosen (or best) version plus, if `with_dependencies`, every
    /// required dependency that isn't already installed. `installed` holds
    /// the project ids already present.
    pub async fn plan_install(
        &self,
        project_id: &str,
        version_id: Option<&str>,
        target: &Target,
        with_dependencies: bool,
        installed: &HashSet<String>,
    ) -> Result<Vec<PlannedMod>> {
        let root = match version_id {
            Some(id) => self.version(id).await?,
            None => {
                let versions = self.compatible_versions(project_id, target).await?;
                match pick_best(&versions) {
                    Some(v) => v.clone(),
                    None => {
                        let title = self.title_of(project_id).await;
                        return Err(Error::Mods(format!("{title} has no version for {}.", target.label)));
                    }
                }
            }
        };

        let mut seen: HashSet<String> = installed.clone();
        seen.insert(root.project_id.clone());
        let mut plan = vec![PlannedMod { version: root, dependency: false }];
        let mut i = 0;
        while with_dependencies && i < plan.len() {
            let deps = plan[i].version.dependencies.clone();
            let needed_by = plan[i].version.project_id.clone();
            i += 1;
            for dep in deps.iter().filter(|d| d.dependency_type == "required") {
                if let Some(pid) = &dep.project_id {
                    if !seen.insert(pid.clone()) {
                        continue;
                    }
                }
                let version = self.dependency_version(dep, target).await?;
                let Some(version) = version else {
                    let dep_title = match &dep.project_id {
                        Some(p) => self.title_of(p).await,
                        None => "a required library".into(),
                    };
                    let parent = self.title_of(&needed_by).await;
                    return Err(Error::Mods(format!(
                        "{parent} needs {dep_title}, which has no version for {}.",
                        target.label
                    )));
                };
                // Dependencies given only by version id: check the project too.
                if dep.project_id.is_none() && !seen.insert(version.project_id.clone()) {
                    continue;
                }
                plan.push(PlannedMod { version, dependency: true });
            }
        }
        Ok(plan)
    }

    async fn dependency_version(&self, dep: &Dependency, target: &Target) -> Result<Option<Version>> {
        // Prefer the newest version that fits the instance; a pinned version
        // id is often for a different loader or game version.
        if let Some(pid) = &dep.project_id {
            let versions = self.compatible_versions(pid, target).await?;
            if let Some(v) = pick_best(&versions) {
                return Ok(Some(v.clone()));
            }
        }
        match &dep.version_id {
            Some(vid) => Ok(Some(self.version(vid).await?)),
            None => Ok(None),
        }
    }

    /// A project's display name for messages; falls back to its id.
    async fn title_of(&self, project_id: &str) -> String {
        self.project(project_id)
            .await
            .map(|p| p.title)
            .unwrap_or_else(|_| project_id.to_string())
    }
}

/// What a project has to fit: a Minecraft version and the loaders it may use.
#[derive(Debug, Clone)]
pub struct Target {
    pub minecraft: String,
    pub loaders: Vec<&'static str>,
    /// For messages, e.g. "Fabric 1.21.4".
    pub label: String,
}

#[derive(Debug, Clone)]
pub struct PlannedMod {
    pub version: Version,
    /// Installed because something else needs it.
    pub dependency: bool,
}

fn search_params(q: &SearchQuery) -> Vec<(&'static str, String)> {
    // Facets: the outer list is AND, each inner list is OR.
    let mut facets: Vec<Vec<String>> = Vec::new();
    if let Some(t) = q.project_type {
        facets.push(vec![format!("project_type:{}", t.slug())]);
    }
    if let Some(v) = &q.game_version {
        facets.push(vec![format!("versions:{v}")]);
    }
    if let Some(loader) = q.loader {
        let loaders = mod_loaders(loader);
        if !loaders.is_empty() {
            facets.push(loaders.iter().map(|l| format!("categories:{l}")).collect());
        }
    }
    for c in &q.categories {
        facets.push(vec![format!("categories:{c}")]);
    }
    let mut params = vec![
        ("query", q.text.trim().to_string()),
        ("index", q.sort.slug().to_string()),
        ("offset", q.offset.to_string()),
        ("limit", q.limit.clamp(1, 100).to_string()),
    ];
    if !facets.is_empty() {
        params.push(("facets", serde_json::to_string(&facets).expect("strings serialize")));
    }
    params
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(id: &str, kind: &str) -> Version {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "project_id": "p",
            "name": id,
            "version_number": id,
            "version_type": kind,
            "date_published": "2026-01-01T00:00:00Z",
            "files": []
        }))
        .unwrap()
    }

    #[test]
    fn best_version_prefers_releases() {
        let list = [version("b2", "beta"), version("r1", "release"), version("r0", "release")];
        assert_eq!(pick_best(&list).unwrap().id, "r1");
        let betas = [version("b2", "beta"), version("b1", "beta")];
        assert_eq!(pick_best(&betas).unwrap().id, "b2");
        assert!(pick_best(&[]).is_none());
    }

    #[test]
    fn search_facets() {
        let q = SearchQuery {
            text: " sodium ".into(),
            project_type: Some(ProjectType::Mod),
            game_version: Some("1.21.4".into()),
            loader: Some(Loader::Quilt),
            limit: 500,
            ..Default::default()
        };
        let params: HashMap<_, _> = search_params(&q).into_iter().collect();
        assert_eq!(params["query"], "sodium");
        assert_eq!(params["limit"], "100");
        assert_eq!(params["index"], "relevance");
        assert_eq!(
            params["facets"],
            r#"[["project_type:mod"],["versions:1.21.4"],["categories:quilt","categories:fabric"]]"#
        );

        let packs = SearchQuery {
            project_type: Some(ProjectType::Modpack),
            ..Default::default()
        };
        let params: HashMap<_, _> = search_params(&packs).into_iter().collect();
        assert_eq!(
            params["facets"],
            r#"[["project_type:modpack"]]"#
        );
    }

    #[test]
    fn urls_escape_segments() {
        let url = Modrinth::url(&["project", "a/b", "version"], &[("loaders", r#"["fabric"]"#.into())]);
        assert_eq!(
            url,
            "https://api.modrinth.com/v2/project/a%2Fb/version?loaders=%5B%22fabric%22%5D"
        );
    }

    #[test]
    fn parses_api_version() {
        let v: Version = serde_json::from_value(serde_json::json!({
            "id": "abc",
            "project_id": "AANobbMI",
            "name": "Sodium 0.6",
            "version_number": "mc1.21.4-0.6.0",
            "version_type": "release",
            "game_versions": ["1.21.4"],
            "loaders": ["fabric"],
            "date_published": "2026-01-01T00:00:00Z",
            "dependencies": [{"project_id": "P7dR8mSH", "version_id": null, "file_name": null, "dependency_type": "required"}],
            "files": [
                {"hashes": {"sha1": "aa", "sha512": "bb"}, "url": "https://cdn.modrinth.com/x.jar", "filename": "x.jar", "primary": false, "size": 3},
                {"hashes": {"sha1": "cc"}, "url": "https://cdn.modrinth.com/y.jar", "filename": "y.jar", "primary": true, "size": 4, "file_type": null}
            ]
        }))
        .unwrap();
        assert_eq!(v.primary_file().unwrap().filename, "y.jar");
        assert_eq!(v.dependencies[0].project_id.as_deref(), Some("P7dR8mSH"));
    }
}
