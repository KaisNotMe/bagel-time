//! CurseForge API v1: searching, project pages, picking files that fit an
//! instance, required dependencies and fingerprint lookups.
//!
//! Results are turned into the same shapes the Modrinth module uses
//! (`SearchHit`, `ProjectDetails`, `Version`) so the app shows both the same
//! way. Ids are CurseForge's numbers as strings.
//!
//! Docs: https://docs.curseforge.com/rest-api/

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

use futures::StreamExt;
use serde::{Deserialize, Serialize};

use crate::download::Downloader;
use crate::error::{Error, Result, parse_json};
use crate::loaders::Loader;
use crate::modrinth::{
    Category, Dependency, FileHashes, GalleryImage, ProjectDetails, ProjectType, SearchHit, SearchQuery,
    SearchResults, SortBy, Target, TeamMember, Version, VersionFile,
};

const API: &str = "https://api.curseforge.com/v1";
/// Minecraft's game id on CurseForge.
pub const GAME_ID: u32 = 432;
/// Overrides the key saved in settings.
pub const KEY_ENV: &str = "BAGEL_CURSEFORGE_KEY";

const NO_KEY: &str = "CurseForge needs an API key. Add yours in Settings.";
/// The API refuses pages that reach past this many results.
const MAX_RESULTS: u32 = 10_000;
const PAGE_SIZE: u32 = 50;
/// How many of a project's files to fetch for its versions list.
const MAX_FILES: u32 = 200;

/// Hosts CurseForge serves files from.
pub const DOWNLOAD_HOSTS: &[&str] = &["edge.forgecdn.net", "mediafilez.forgecdn.net", "media.forgecdn.net"];

pub fn class_id(t: ProjectType) -> u32 {
    match t {
        ProjectType::Mod => 6,
        ProjectType::Modpack => 4471,
        ProjectType::ResourcePack => 12,
        ProjectType::Shader => 6552,
    }
}

pub fn project_type(class_id: u32) -> Option<ProjectType> {
    match class_id {
        6 => Some(ProjectType::Mod),
        4471 => Some(ProjectType::Modpack),
        12 => Some(ProjectType::ResourcePack),
        6552 => Some(ProjectType::Shader),
        _ => None,
    }
}

/// CurseForge's ModLoaderType number.
fn loader_type(loader: Loader) -> Option<u32> {
    match loader {
        Loader::Vanilla => None,
        Loader::Forge => Some(1),
        Loader::Fabric => Some(4),
        Loader::Quilt => Some(5),
        Loader::NeoForge => Some(6),
    }
}

/// Loader names (as Modrinth writes them) that CurseForge puts in a file's
/// game version list.
const LOADER_TAGS: &[(&str, &str)] = &[("Forge", "forge"), ("NeoForge", "neoforge"), ("Fabric", "fabric"), ("Quilt", "quilt")];

fn sort_field(sort: SortBy) -> u32 {
    // 2 Popularity, 3 LastUpdated, 6 TotalDownloads, 11 ReleasedDate,
    // 12 Rating. CurseForge has no relevance order; popularity is closest.
    match sort {
        SortBy::Relevance => 2,
        SortBy::Downloads => 6,
        SortBy::Follows => 12,
        SortBy::Newest => 11,
        SortBy::Updated => 3,
    }
}

#[derive(Deserialize)]
struct Data<T> {
    data: T,
}

#[derive(Deserialize)]
struct Paged<T> {
    data: Vec<T>,
    pagination: Pagination,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Pagination {
    total_count: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CfMod {
    pub id: u64,
    pub name: String,
    pub slug: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub download_count: f64,
    #[serde(default)]
    pub thumbs_up_count: u64,
    #[serde(default)]
    pub links: Links,
    #[serde(default)]
    pub authors: Vec<Author>,
    #[serde(default)]
    pub logo: Option<Asset>,
    #[serde(default)]
    pub screenshots: Vec<Asset>,
    #[serde(default)]
    pub categories: Vec<CfCategory>,
    #[serde(default)]
    pub class_id: Option<u32>,
    #[serde(default)]
    pub date_created: String,
    #[serde(default)]
    pub date_modified: String,
    #[serde(default)]
    pub latest_files_indexes: Vec<FileIndex>,
    #[serde(default)]
    pub allow_mod_distribution: Option<bool>,
}

impl CfMod {
    pub fn icon_url(&self) -> Option<String> {
        self.logo.as_ref().map(|l| l.thumbnail_url.clone()).filter(|u| !u.is_empty())
    }

    /// The project's page on curseforge.com.
    pub fn website_url(&self) -> Option<String> {
        non_empty(&self.links.website_url)
    }

    pub fn downloads_blocked(&self) -> bool {
        self.allow_mod_distribution == Some(false)
    }

    fn project_type(&self) -> ProjectType {
        self.class_id.and_then(project_type).unwrap_or(ProjectType::Mod)
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Links {
    #[serde(default)]
    pub website_url: Option<String>,
    #[serde(default)]
    pub wiki_url: Option<String>,
    #[serde(default)]
    pub issues_url: Option<String>,
    #[serde(default)]
    pub source_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Author {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub thumbnail_url: String,
    #[serde(default)]
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CfCategory {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub class_id: Option<u32>,
    #[serde(default)]
    pub is_class: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileIndex {
    pub game_version: String,
    #[serde(default)]
    pub mod_loader: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CfFile {
    pub id: u64,
    pub mod_id: u64,
    pub display_name: String,
    pub file_name: String,
    /// 1 release, 2 beta, 3 alpha.
    pub release_type: u32,
    pub file_date: String,
    #[serde(default)]
    pub file_length: u64,
    #[serde(default)]
    pub download_count: u64,
    /// Missing when the author only allows downloads from the website.
    #[serde(default)]
    pub download_url: Option<String>,
    #[serde(default)]
    pub game_versions: Vec<String>,
    #[serde(default)]
    pub hashes: Vec<FileHash>,
    #[serde(default)]
    pub dependencies: Vec<FileDependency>,
    #[serde(default)]
    pub file_fingerprint: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FileHash {
    pub value: String,
    /// 1 SHA-1, 2 MD5.
    pub algo: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDependency {
    pub mod_id: u64,
    /// 1 embedded, 2 optional, 3 required, 4 tool, 5 incompatible, 6 include.
    pub relation_type: u32,
}

impl CfFile {
    pub fn sha1(&self) -> Option<String> {
        self.hashes.iter().find(|h| h.algo == 1).map(|h| h.value.to_ascii_lowercase())
    }

    /// Minecraft versions, without the loader and side tags CurseForge mixes in.
    pub fn minecraft_versions(&self) -> Vec<String> {
        self.game_versions
            .iter()
            .filter(|v| v.starts_with(|c: char| c.is_ascii_digit()))
            .cloned()
            .collect()
    }

    /// Loaders, as Modrinth names them.
    pub fn loaders(&self) -> Vec<&'static str> {
        LOADER_TAGS
            .iter()
            .filter(|(tag, _)| self.game_versions.iter().any(|v| v.eq_ignore_ascii_case(tag)))
            .map(|(_, name)| *name)
            .collect()
    }

    pub fn version_type(&self) -> &'static str {
        match self.release_type {
            2 => "beta",
            3 => "alpha",
            _ => "release",
        }
    }

    /// Whether the file runs in an instance. Files without loader tags (old
    /// Forge mods, resource packs) only have to match the Minecraft version.
    pub fn fits(&self, target: &Target) -> bool {
        if !self.game_versions.iter().any(|v| v == &target.minecraft) {
            return false;
        }
        let wanted: Vec<&str> = target
            .loaders
            .iter()
            .copied()
            .filter(|l| LOADER_TAGS.iter().any(|(_, n)| n == l))
            .collect();
        let has = self.loaders();
        wanted.is_empty() || has.is_empty() || has.iter().any(|l| wanted.contains(l))
    }

    /// The file as a Modrinth-style version. `url` is empty when the
    /// author blocks third-party downloads.
    pub fn to_version(&self) -> Version {
        Version {
            id: self.id.to_string(),
            project_id: self.mod_id.to_string(),
            name: self.display_name.clone(),
            version_number: self.display_name.clone(),
            version_type: self.version_type().to_string(),
            game_versions: self.minecraft_versions(),
            loaders: self.loaders().into_iter().map(String::from).collect(),
            date_published: self.file_date.clone(),
            downloads: self.download_count,
            changelog: None,
            dependencies: self
                .dependencies
                .iter()
                .map(|d| Dependency {
                    version_id: None,
                    project_id: Some(d.mod_id.to_string()),
                    dependency_type: match d.relation_type {
                        1 | 6 => "embedded",
                        2 | 4 => "optional",
                        3 => "required",
                        _ => "incompatible",
                    }
                    .to_string(),
                })
                .collect(),
            files: vec![VersionFile {
                hashes: FileHashes {
                    sha1: self.sha1().unwrap_or_default(),
                    sha512: None,
                },
                url: self.download_url.clone().unwrap_or_default(),
                filename: self.file_name.clone(),
                primary: true,
                size: self.file_length,
            }],
        }
    }
}

/// Newest release if there is one, else the newest file. Expects newest first.
pub fn pick_best(files: &[CfFile]) -> Option<&CfFile> {
    files.iter().find(|f| f.release_type == 1).or(files.first())
}

fn non_empty(s: &Option<String>) -> Option<String> {
    s.as_ref().filter(|s| !s.trim().is_empty()).cloned()
}

/// A file picked for install.
#[derive(Debug, Clone)]
pub struct PlannedFile {
    pub file: CfFile,
    /// Installed because something else needs it.
    pub dependency: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FingerprintMatches {
    #[serde(default)]
    exact_matches: Vec<FingerprintMatch>,
}

#[derive(Debug, Clone, Deserialize)]
struct FingerprintMatch {
    file: CfFile,
}

#[derive(Clone, Default)]
pub struct CurseForge {
    dl: Downloader,
    key: Arc<RwLock<String>>,
}

impl CurseForge {
    pub fn new(key: &str) -> Self {
        let cf = Self::default();
        cf.set_key(key);
        cf
    }

    pub fn downloader(&self) -> &Downloader {
        &self.dl
    }

    /// Uses a new key from settings. The environment variable still wins.
    pub fn set_key(&self, key: &str) {
        *self.key.write().expect("key lock") = key.trim().to_string();
    }

    /// Settings first (so a key the player pastes always wins), then the
    /// environment at run time, then at build time (so release builds work
    /// without setup and the key never sits in the repository).
    fn key(&self) -> Option<String> {
        let clean = |k: &str| Some(k.trim().to_string()).filter(|k| !k.is_empty());
        clean(&self.key.read().expect("key lock"))
            .or_else(|| std::env::var(KEY_ENV).ok().and_then(|k| clean(&k)))
            .or_else(|| option_env!("BAGEL_CURSEFORGE_KEY").and_then(clean))
    }

    /// Whether this build has a key built in.
    pub fn has_builtin_key() -> bool {
        option_env!("BAGEL_CURSEFORGE_KEY").is_some_and(|k| !k.trim().is_empty())
    }

    pub fn has_key(&self) -> bool {
        self.key().is_some()
    }

    /// Whether the key comes from the environment rather than settings.
    pub fn key_from_env() -> bool {
        std::env::var(KEY_ENV).is_ok_and(|k| !k.trim().is_empty())
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

    async fn send<T: serde::de::DeserializeOwned>(&self, req: reqwest::RequestBuilder, url: &str) -> Result<T> {
        let key = self.key().ok_or_else(|| Error::Mods(NO_KEY.into()))?;
        let resp = req
            .header("x-api-key", key)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await?;
        if resp.status() == reqwest::StatusCode::FORBIDDEN || resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(Error::Mods(
                "CurseForge says the API key is missing or invalid. Get a new one at console.curseforge.com and paste it in Settings.".into(),
            ));
        }
        let resp = resp.error_for_status()?;
        parse_json(&resp.bytes().await?, url)
    }

    async fn get<T: serde::de::DeserializeOwned>(&self, segments: &[&str], query: &[(&str, String)]) -> Result<T> {
        let url = Self::url(segments, query);
        self.send(self.dl.client().get(&url), &url).await
    }

    async fn post<B: Serialize + ?Sized, T: serde::de::DeserializeOwned>(&self, segments: &[&str], body: &B) -> Result<T> {
        let url = Self::url(segments, &[]);
        self.send(self.dl.client().post(&url).json(body), &url).await
    }

    pub async fn search(&self, q: &SearchQuery) -> Result<SearchResults> {
        let limit = q.limit.clamp(1, PAGE_SIZE);
        let offset = q.offset.min(MAX_RESULTS - limit);
        let page: Paged<CfMod> = self.get(&["mods", "search"], &search_params(q, offset, limit)).await?;
        Ok(SearchResults {
            hits: page.data.iter().map(to_hit).collect(),
            offset,
            limit,
            total_hits: page.pagination.total_count.min(MAX_RESULTS as u64) as u32,
        })
    }

    pub async fn get_mod(&self, id: &str) -> Result<CfMod> {
        let r: Data<CfMod> = self.get(&["mods", id], &[]).await?;
        Ok(r.data)
    }

    /// Several projects at once; unknown ids are left out.
    pub async fn mods(&self, ids: &[u64]) -> Result<Vec<CfMod>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            mod_ids: &'a [u64],
            filter_pc_only: bool,
        }
        let mut out = Vec::new();
        for chunk in ids.chunks(500) {
            let r: Data<Vec<CfMod>> = self.post(&["mods"], &Body { mod_ids: chunk, filter_pc_only: true }).await?;
            out.extend(r.data);
        }
        Ok(out)
    }

    /// Files by id, in any order; unknown ids are left out.
    pub async fn files(&self, ids: &[u64]) -> Result<Vec<CfFile>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            file_ids: &'a [u64],
        }
        let mut out = Vec::new();
        for chunk in ids.chunks(500) {
            let r: Data<Vec<CfFile>> = self.post(&["mods", "files"], &Body { file_ids: chunk }).await?;
            out.extend(r.data);
        }
        Ok(out)
    }

    pub async fn file(&self, mod_id: &str, file_id: &str) -> Result<CfFile> {
        let r: Data<CfFile> = self.get(&["mods", mod_id, "files", file_id], &[]).await?;
        Ok(r.data)
    }

    pub async fn project_details(&self, id: &str) -> Result<ProjectDetails> {
        let (m, body) = futures::try_join!(self.get_mod(id), self.description(id))?;
        Ok(to_details(&m, body))
    }

    /// The project page's description, as HTML.
    async fn description(&self, id: &str) -> Result<String> {
        let r: Data<String> = self.get(&["mods", id, "description"], &[]).await?;
        Ok(r.data)
    }

    pub async fn members(&self, id: &str) -> Result<Vec<TeamMember>> {
        let m = self.get_mod(id).await?;
        Ok(m.authors
            .into_iter()
            .enumerate()
            .map(|(i, a)| TeamMember {
                username: a.name,
                avatar_url: None,
                role: if i == 0 { "Owner".into() } else { "Author".into() },
            })
            .collect())
    }

    /// Search filter categories for every project type we show.
    pub async fn categories(&self) -> Result<Vec<Category>> {
        let r: Data<Vec<CfCategory>> = self.get(&["categories"], &[("gameId", GAME_ID.to_string())]).await?;
        Ok(r.data
            .into_iter()
            .filter(|c| c.is_class != Some(true))
            .filter_map(|c| {
                let t = project_type(c.class_id?)?;
                Some(Category {
                    icon: String::new(),
                    name: c.id.to_string(),
                    project_type: t.slug().to_string(),
                    header: "categories".into(),
                    label: Some(c.name),
                    icon_url: c.icon_url,
                })
            })
            .collect())
    }

    /// A project's files, newest first, up to `MAX_FILES`.
    pub async fn project_files(&self, mod_id: &str, game_version: Option<&str>, loader: Option<Loader>) -> Result<Vec<CfFile>> {
        let mut out: Vec<CfFile> = Vec::new();
        let mut index = 0;
        loop {
            let mut query = vec![("index", index.to_string()), ("pageSize", PAGE_SIZE.to_string())];
            if let Some(v) = game_version {
                query.push(("gameVersion", v.to_string()));
            }
            if let Some(t) = loader.and_then(loader_type) {
                query.push(("modLoaderType", t.to_string()));
            }
            let page: Paged<CfFile> = self.get(&["mods", mod_id, "files"], &query).await?;
            let got = page.data.len() as u32;
            out.extend(page.data);
            index += got;
            if got < PAGE_SIZE || index >= MAX_FILES || index as u64 >= page.pagination.total_count {
                break;
            }
        }
        out.sort_by(|a, b| b.file_date.cmp(&a.file_date));
        Ok(out)
    }

    /// Versions for the project page. Empty filters mean "any".
    pub async fn project_versions(&self, mod_id: &str, loaders: &[&str], game_versions: &[&str]) -> Result<Vec<Version>> {
        let files = self.project_files(mod_id, game_versions.first().copied(), None).await?;
        Ok(files
            .iter()
            .filter(|f| game_versions.is_empty() || game_versions.iter().any(|v| f.game_versions.iter().any(|g| g == v)))
            .filter(|f| {
                loaders.is_empty() || {
                    let has = f.loaders();
                    has.is_empty() || has.iter().any(|l| loaders.contains(l))
                }
            })
            .map(CfFile::to_version)
            .collect())
    }

    /// Files that fit a target, newest first. Loaders are checked here
    /// rather than by the API, which drops old files without loader tags.
    pub async fn compatible_files(&self, mod_id: &str, target: &Target) -> Result<Vec<CfFile>> {
        let files = self.project_files(mod_id, Some(&target.minecraft), None).await?;
        Ok(files.into_iter().filter(|f| f.fits(target)).collect())
    }

    /// What to download to install a project: the chosen (or best) file
    /// plus, if `with_dependencies`, each required dependency not yet in
    /// `installed` (project ids).
    pub async fn plan_install(
        &self,
        mod_id: &str,
        file_id: Option<&str>,
        target: &Target,
        with_dependencies: bool,
        installed: &HashSet<String>,
    ) -> Result<Vec<PlannedFile>> {
        let root = match file_id {
            Some(id) => {
                let file = self.file(mod_id, id).await?;
                if !file.fits(target) {
                    let title = self.title_of(mod_id).await;
                    return Err(Error::Mods(format!(
                        "{title} {} isn't made for {}.",
                        file.display_name, target.label
                    )));
                }
                file
            }
            None => {
                let files = self.compatible_files(mod_id, target).await?;
                match pick_best(&files) {
                    Some(f) => f.clone(),
                    None => {
                        let title = self.title_of(mod_id).await;
                        return Err(Error::Mods(format!("{title} has no file for {}.", target.label)));
                    }
                }
            }
        };
        let mut seen = installed.clone();
        seen.insert(root.mod_id.to_string());
        let mut plan = vec![PlannedFile { file: root, dependency: false }];
        let mut i = 0;
        while with_dependencies && i < plan.len() {
            let deps: Vec<u64> = plan[i]
                .file
                .dependencies
                .iter()
                .filter(|d| d.relation_type == 3)
                .map(|d| d.mod_id)
                .collect();
            let parent = plan[i].file.mod_id;
            i += 1;
            for dep in deps {
                if !seen.insert(dep.to_string()) {
                    continue;
                }
                let files = self.compatible_files(&dep.to_string(), target).await?;
                let Some(file) = pick_best(&files) else {
                    let (dep_title, parent_title) =
                        (self.title_of(&dep.to_string()).await, self.title_of(&parent.to_string()).await);
                    return Err(Error::Mods(format!(
                        "{parent_title} needs {dep_title}, which has no file for {}.",
                        target.label
                    )));
                };
                plan.push(PlannedFile { file: file.clone(), dependency: true });
            }
        }
        Ok(plan)
    }

    async fn title_of(&self, mod_id: &str) -> String {
        self.get_mod(mod_id).await.map(|m| m.name).unwrap_or_else(|_| mod_id.to_string())
    }

    /// Looks files up by fingerprint. Returns fingerprint -> file for the
    /// ones CurseForge knows.
    pub async fn files_by_fingerprint(&self, fingerprints: &[u32]) -> Result<HashMap<u32, CfFile>> {
        if fingerprints.is_empty() {
            return Ok(HashMap::new());
        }
        #[derive(Serialize)]
        struct Body<'a> {
            fingerprints: &'a [u32],
        }
        let r: Data<FingerprintMatches> = self
            .post(&["fingerprints", &GAME_ID.to_string()], &Body { fingerprints })
            .await?;
        Ok(r.data
            .exact_matches
            .into_iter()
            .map(|m| (m.file.file_fingerprint as u32, m.file))
            .collect())
    }

    /// The newest compatible file for each project, when it differs from
    /// the installed one. Takes (project id, installed file id) pairs.
    pub async fn updates(&self, installed: &[(String, String)], target: &Target) -> Result<HashMap<String, CfFile>> {
        // Owned items: closures over borrowed ones aren't `Send` enough for
        // Tauri commands.
        let results: Vec<Result<Option<(String, CfFile)>>> = futures::stream::iter(installed.to_vec())
            .map(|(mod_id, file_id)| async move {
                let files = self.compatible_files(&mod_id, target).await?;
                let newer = pick_best(&files).filter(|f| f.id.to_string() != file_id).cloned();
                Ok(newer.map(|f| (mod_id, f)))
            })
            .buffer_unordered(8)
            .collect()
            .await;
        let mut out = HashMap::new();
        for r in results {
            if let Some((id, file)) = r? {
                out.insert(id, file);
            }
        }
        Ok(out)
    }
}

fn search_params(q: &SearchQuery, offset: u32, limit: u32) -> Vec<(&'static str, String)> {
    let mut params = vec![
        ("gameId", GAME_ID.to_string()),
        ("sortField", sort_field(q.sort).to_string()),
        ("sortOrder", "desc".to_string()),
        ("index", offset.to_string()),
        ("pageSize", limit.to_string()),
    ];
    if let Some(t) = q.project_type {
        params.push(("classId", class_id(t).to_string()));
    }
    let text = q.text.trim();
    if !text.is_empty() {
        params.push(("searchFilter", text.to_string()));
    }
    if let Some(v) = &q.game_version {
        params.push(("gameVersion", v.clone()));
    }
    if let Some(loader) = q.loader {
        // Quilt runs Fabric mods too.
        let types: Vec<u32> = match loader {
            Loader::Quilt => vec![5, 4],
            l => loader_type(l).into_iter().collect(),
        };
        if !types.is_empty() {
            params.push(("modLoaderTypes", serde_json::to_string(&types).expect("numbers serialize")));
        }
    }
    if !q.categories.is_empty() {
        let ids: Vec<u64> = q.categories.iter().filter_map(|c| c.parse().ok()).take(10).collect();
        params.push(("categoryIds", serde_json::to_string(&ids).expect("numbers serialize")));
    }
    params
}

fn to_hit(m: &CfMod) -> SearchHit {
    SearchHit {
        project_id: m.id.to_string(),
        slug: m.slug.clone(),
        title: m.name.clone(),
        description: m.summary.clone(),
        author: m.authors.first().map(|a| a.name.clone()).unwrap_or_default(),
        icon_url: m.icon_url(),
        downloads: m.download_count as u64,
        follows: m.thumbs_up_count,
        display_categories: m.categories.iter().map(|c| c.name.clone()).collect(),
        project_type: m.project_type().slug().to_string(),
        downloads_blocked: m.downloads_blocked(),
    }
}

fn to_details(m: &CfMod, body: String) -> ProjectDetails {
    let mut game_versions: Vec<String> = Vec::new();
    let mut loaders: Vec<String> = Vec::new();
    for f in &m.latest_files_indexes {
        if !game_versions.contains(&f.game_version) {
            game_versions.push(f.game_version.clone());
        }
        let loader = match f.mod_loader {
            Some(1) => Some("forge"),
            Some(4) => Some("fabric"),
            Some(5) => Some("quilt"),
            Some(6) => Some("neoforge"),
            _ => None,
        };
        if let Some(l) = loader
            && !loaders.iter().any(|x| x == l)
        {
            loaders.push(l.to_string());
        }
    }
    // The page lists versions oldest first.
    game_versions.sort_by(|a, b| crate::loaders::compare_versions(a, b));
    ProjectDetails {
        id: m.id.to_string(),
        slug: m.slug.clone(),
        title: m.name.clone(),
        description: m.summary.clone(),
        body,
        project_type: m.project_type().slug().to_string(),
        icon_url: m.icon_url(),
        downloads: m.download_count as u64,
        followers: m.thumbs_up_count,
        categories: m.categories.iter().map(|c| c.name.clone()).collect(),
        additional_categories: Vec::new(),
        game_versions,
        loaders,
        published: m.date_created.clone(),
        updated: m.date_modified.clone(),
        license: None,
        client_side: None,
        server_side: None,
        source_url: non_empty(&m.links.source_url),
        issues_url: non_empty(&m.links.issues_url),
        wiki_url: non_empty(&m.links.wiki_url),
        discord_url: None,
        gallery: m
            .screenshots
            .iter()
            .enumerate()
            .map(|(i, s)| GalleryImage {
                url: s.url.clone(),
                featured: false,
                title: s.title.clone().filter(|t| !t.is_empty()),
                description: s.description.clone().filter(|t| !t.is_empty()),
                ordering: i as i64,
            })
            .collect(),
        website_url: m.website_url(),
        downloads_blocked: m.downloads_blocked(),
    }
}

/// CurseForge's file fingerprint: 32-bit MurmurHash2 (seed 1) of the file
/// with all tabs, newlines, carriage returns and spaces removed.
pub fn fingerprint(bytes: &[u8]) -> u32 {
    let data: Vec<u8> = bytes
        .iter()
        .copied()
        .filter(|b| !matches!(b, 9 | 10 | 13 | 32))
        .collect();
    murmur2(&data, 1)
}

fn murmur2(data: &[u8], seed: u32) -> u32 {
    const M: u32 = 0x5bd1_e995;
    let mut h = seed ^ data.len() as u32;
    let mut chunks = data.chunks_exact(4);
    for c in &mut chunks {
        let mut k = u32::from_le_bytes([c[0], c[1], c[2], c[3]]);
        k = k.wrapping_mul(M);
        k ^= k >> 24;
        k = k.wrapping_mul(M);
        h = h.wrapping_mul(M) ^ k;
    }
    let rest = chunks.remainder();
    if !rest.is_empty() {
        for (i, b) in rest.iter().enumerate() {
            h ^= (*b as u32) << (8 * i);
        }
        h = h.wrapping_mul(M);
    }
    h ^= h >> 13;
    h = h.wrapping_mul(M);
    h ^= h >> 15;
    h
}

pub async fn fingerprint_file(path: &std::path::Path) -> Result<u32> {
    use crate::error::IoContext;
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || Ok(fingerprint(&std::fs::read(&path).at(&path)?))).await?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(game_versions: &[&str]) -> CfFile {
        serde_json::from_value(serde_json::json!({
            "id": 5,
            "modId": 9,
            "displayName": "Thing 1.0",
            "fileName": "thing-1.0.jar",
            "releaseType": 1,
            "fileDate": "2026-01-01T00:00:00Z",
            "fileLength": 10,
            "downloadUrl": null,
            "gameVersions": game_versions,
            "hashes": [{"value": "ABC", "algo": 1}, {"value": "x", "algo": 2}],
            "dependencies": [{"modId": 7, "relationType": 3}, {"modId": 8, "relationType": 2}],
            "fileFingerprint": 4000000000u64
        }))
        .unwrap()
    }

    fn target(minecraft: &str, loaders: &[&'static str]) -> Target {
        Target {
            minecraft: minecraft.into(),
            loaders: loaders.to_vec(),
            label: String::new(),
        }
    }

    #[test]
    fn fingerprints_ignore_whitespace() {
        assert_eq!(fingerprint(b"a b\r\n\tc"), fingerprint(b"abc"));
        assert_ne!(fingerprint(b"abc"), fingerprint(b"abd"));
        // Reference values from MurmurHash2 with seed 1.
        assert_eq!(murmur2(b"", 1), 0x5bd15e36);
    }

    #[test]
    fn files_map_to_versions() {
        let f = file(&["1.20.1", "Forge", "Client", "NeoForge"]);
        assert_eq!(f.minecraft_versions(), ["1.20.1"]);
        assert_eq!(f.loaders(), ["forge", "neoforge"]);
        let v = f.to_version();
        assert_eq!(v.id, "5");
        assert_eq!(v.project_id, "9");
        assert_eq!(v.files[0].url, "");
        assert_eq!(v.files[0].hashes.sha1, "abc");
        assert_eq!(v.dependencies[0].dependency_type, "required");
        assert_eq!(v.dependencies[1].dependency_type, "optional");
    }

    #[test]
    fn fitting_files() {
        let forge = file(&["1.20.1", "Forge"]);
        assert!(forge.fits(&target("1.20.1", &["forge"])));
        assert!(!forge.fits(&target("1.20.1", &["fabric"])));
        assert!(!forge.fits(&target("1.21.1", &["forge"])));
        // Old files without loader tags, and resource packs.
        let untagged = file(&["1.7.10"]);
        assert!(untagged.fits(&target("1.7.10", &["forge"])));
        assert!(untagged.fits(&target("1.7.10", &["minecraft"])));
        let fabric = file(&["1.21.1", "Fabric"]);
        assert!(fabric.fits(&target("1.21.1", &["quilt", "fabric"])));
    }

    #[test]
    fn search_parameters() {
        let q = SearchQuery {
            text: " jei ".into(),
            project_type: Some(ProjectType::Mod),
            game_version: Some("1.20.1".into()),
            loader: Some(Loader::Quilt),
            categories: vec!["423".into(), "nope".into()],
            sort: SortBy::Downloads,
            offset: 20,
            limit: 20,
        };
        let p: HashMap<_, _> = search_params(&q, 20, 20).into_iter().collect();
        assert_eq!(p["searchFilter"], "jei");
        assert_eq!(p["classId"], "6");
        assert_eq!(p["modLoaderTypes"], "[5,4]");
        assert_eq!(p["categoryIds"], "[423]");
        assert_eq!(p["sortField"], "6");
        assert_eq!(p["index"], "20");
    }
}
