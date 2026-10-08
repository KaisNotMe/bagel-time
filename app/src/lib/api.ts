// Typed wrappers around the Rust commands in src-tauri/src (commands.rs,
// content.rs, accounts.rs). Browsing calls take a `source`; results look the
// same for Modrinth and CurseForge.
import { convertFileSrc, invoke } from "@tauri-apps/api/core";

export type Loader = "vanilla" | "fabric" | "quilt" | "forge" | "neoforge";

export const LOADER_NAMES: Record<Loader, string> = {
  vanilla: "Vanilla",
  fabric: "Fabric",
  quilt: "Quilt",
  forge: "Forge",
  neoforge: "NeoForge",
};

export type LoaderVersion = { version: string; stable: boolean };

export type Instance = {
  id: string;
  name: string;
  gameVersion: string;
  loader: Loader;
  loaderVersion: string | null;
  memoryMb: number | null;
  javaArgs: string | null;
  icon: string | null;
  /** Absolute path of the icon file, if any. */
  iconPath: string | null;
  /** Unix seconds. */
  created: number;
  lastPlayed: number | null;
  running: boolean;
};

export type InstancePatch = { name: string; memoryMb: number | null; javaArgs: string | null };

export type VersionInfo = {
  id: string;
  kind: "release" | "snapshot";
  releaseTime: string;
};

export type Settings = {
  offlineUsername: string;
  memoryMb: number;
  showSnapshots: boolean;
  javaArgs: string;
  curseforgeApiKey: string;
};

export type LogLine = {
  level: string;
  thread: string | null;
  /** Unix milliseconds. */
  timestamp: number | null;
  message: string;
};

export type AccountSummary = { uuid: string; username: string };
export type AccountList = { accounts: AccountSummary[]; active: string | null };
export type DeviceLogin = { userCode: string; verificationUri: string; expiresIn: number };

export type Source = "modrinth" | "curseforge";

export const SOURCE_NAMES: Record<Source, string> = { modrinth: "Modrinth", curseforge: "CurseForge" };

export type ProjectType = "mod" | "modpack" | "resourcepack" | "shader";
export type ContentKind = "mod" | "resourcepack" | "shader";
export type SortBy = "relevance" | "downloads" | "follows" | "newest" | "updated";

export const PROJECT_TYPE_NAMES: Record<ProjectType, { one: string; many: string }> = {
  mod: { one: "Mod", many: "Mods" },
  modpack: { one: "Modpack", many: "Modpacks" },
  resourcepack: { one: "Resource pack", many: "Resource packs" },
  shader: { one: "Shader", many: "Shaders" },
};

/** Which kinds of content an instance with this loader can use. */
export function supportedKinds(loader: Loader): ContentKind[] {
  return loader === "vanilla" ? ["resourcepack"] : ["mod", "resourcepack", "shader"];
}

export type SearchHit = {
  projectId: string;
  slug: string;
  title: string;
  description: string;
  author: string;
  iconUrl: string | null;
  downloads: number;
  follows: number;
  displayCategories: string[];
  projectType: string;
  /** CurseForge: the author only allows downloads from the website. */
  downloadsBlocked: boolean;
};

export type SearchResults = { hits: SearchHit[]; offset: number; limit: number; totalHits: number };

export type SearchArgs = {
  source: Source;
  text: string;
  projectType: ProjectType;
  gameVersion: string | null;
  loader: Loader | null;
  categories: string[];
  sort: SortBy;
  offset: number;
  limit: number;
};

export type GalleryImage = {
  url: string;
  featured: boolean;
  title: string | null;
  description: string | null;
  ordering: number;
};

export type ProjectDetails = {
  id: string;
  slug: string;
  title: string;
  description: string;
  body: string;
  projectType: ProjectType;
  iconUrl: string | null;
  downloads: number;
  followers: number;
  categories: string[];
  additionalCategories: string[];
  gameVersions: string[];
  loaders: string[];
  published: string;
  updated: string;
  license: { id: string; name: string } | null;
  clientSide: string | null;
  serverSide: string | null;
  sourceUrl: string | null;
  issuesUrl: string | null;
  wikiUrl: string | null;
  discordUrl: string | null;
  gallery: GalleryImage[];
  /** The project page, when the site gives one (CurseForge). */
  websiteUrl: string | null;
  downloadsBlocked: boolean;
};

export type TeamMember = { username: string; avatarUrl: string | null; role: string };

export type ProjectVersion = {
  id: string;
  projectId: string;
  name: string;
  versionNumber: string;
  versionType: "release" | "beta" | "alpha";
  gameVersions: string[];
  loaders: string[];
  datePublished: string;
  downloads: number;
  changelog: string | null;
  files: { filename: string; size: number; primary: boolean; url: string }[];
};

export type Category = {
  /** Inline SVG (Modrinth). */
  icon: string;
  /** Filter value: a slug (Modrinth) or an id (CurseForge). */
  name: string;
  projectType: string;
  header: string;
  /** Display name when `name` is an id. */
  label: string | null;
  iconUrl: string | null;
};

/** A server from an instance's list (the same list the game shows). */
export type Server = {
  name: string;
  address: string;
  /** `data:` URL saved by the game, if any. */
  icon: string | null;
};

export type ServerStatus = {
  /** Message of the day with `§` formatting codes. */
  motd: string;
  online: number;
  max: number;
  sample: string[];
  version: string;
  protocol: number;
  icon: string | null;
  pingMs: number;
};

export type RecentServer = {
  instanceId: string;
  name: string;
  address: string;
  lastPlayed: number;
};

/** Whether a project fits an instance, and which version would go in. */
export type Fit = {
  fits: boolean;
  version: string | null;
  reason: string | null;
};

export type InstalledContent = {
  kind: ContentKind;
  /** Without the .disabled suffix; identifies the file in calls. */
  fileName: string;
  enabled: boolean;
  size: number;
  title: string;
  source: Source;
  projectId: string | null;
  versionId: string | null;
  versionNumber: string | null;
  iconUrl: string | null;
  dependency: boolean;
};

export type ContentUpdate = {
  kind: ContentKind;
  fileName: string;
  source: Source;
  projectId: string;
  versionId: string;
  versionNumber: string;
};

export type ManualDownload = { title: string; fileName: string; url: string; folder: string };
/** A new instance from a modpack, and files the player has to download by hand. */
export type PackResult = { instance: Instance; manual: ManualDownload[] };
export type CurseForgeStatus = { hasKey: boolean; managed: boolean };

export type World = {
  folder: string;
  name: string;
  lastPlayed: number | null;
  gameMode: string | null;
  hardcore: boolean;
  version: string | null;
  icon: string | null;
};

export type Screenshot = { fileName: string; path: string; modified: number; size: number };
export type LogFile = { name: string; crashReport: boolean; modified: number; size: number };

export const api = {
  listAccounts: () => invoke<AccountList>("list_accounts"),
  startLogin: () => invoke<DeviceLogin>("start_login"),
  openLoginPage: () => invoke<void>("open_login_page"),
  cancelLogin: () => invoke<void>("cancel_login"),
  setActiveAccount: (uuid: string | null) => invoke<AccountList>("set_active_account", { uuid }),
  removeAccount: (uuid: string) => invoke<AccountList>("remove_account", { uuid }),

  listVersions: (snapshots: boolean) => invoke<VersionInfo[]>("list_versions", { snapshots }),
  listLoaderVersions: (loader: Loader, gameVersion: string) =>
    invoke<LoaderVersion[]>("list_loader_versions", { loader, gameVersion }),

  listInstances: () => invoke<Instance[]>("list_instances"),
  getInstance: (id: string) => invoke<Instance>("get_instance", { id }),
  createInstance: (name: string, gameVersion: string, loader: Loader, loaderVersion: string | null) =>
    invoke<Instance>("create_instance", { name, gameVersion, loader, loaderVersion }),
  updateInstance: (id: string, patch: InstancePatch) => invoke<Instance>("update_instance", { id, patch }),
  changeInstanceVersion: (id: string, gameVersion: string, loader: Loader, loaderVersion: string | null) =>
    invoke<Instance>("change_instance_version", { id, gameVersion, loader, loaderVersion }),
  duplicateInstance: (id: string, name: string) => invoke<Instance>("duplicate_instance", { id, name }),
  setInstanceIcon: (id: string, path: string) => invoke<Instance>("set_instance_icon", { id, path }),
  clearInstanceIcon: (id: string) => invoke<Instance>("clear_instance_icon", { id }),
  deleteInstance: (id: string) => invoke<void>("delete_instance", { id }),
  /** `sub` is a folder inside the game folder, e.g. "mods" or "saves/My World". */
  openInstanceFolder: (id: string, sub: string | null = null) => invoke<void>("open_instance_folder", { id, sub }),
  launchInstance: (id: string, server: string | null = null) => invoke<void>("launch_instance", { id, server }),
  listServers: (id: string) => invoke<Server[]>("list_servers", { id }),
  addServer: (id: string, name: string, address: string) => invoke<void>("add_server", { id, name, address }),
  removeServer: (id: string, address: string) => invoke<void>("remove_server", { id, address }),
  pingServer: (address: string) => invoke<ServerStatus>("ping_server", { address }),
  recentServers: () => invoke<RecentServer[]>("recent_servers"),
  stopInstance: (id: string) => invoke<boolean>("stop_instance", { id }),

  listWorlds: (id: string) => invoke<World[]>("list_worlds", { id }),
  listScreenshots: (id: string) => invoke<Screenshot[]>("list_screenshots", { id }),
  openScreenshot: (id: string, fileName: string) => invoke<void>("open_screenshot", { id, fileName }),
  deleteScreenshot: (id: string, fileName: string) => invoke<void>("delete_screenshot", { id, fileName }),
  listLogFiles: (id: string) => invoke<LogFile[]>("list_log_files", { id }),
  readLogFile: (id: string, name: string) => invoke<string>("read_log_file", { id, name }),

  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),

  searchProjects: (args: SearchArgs) => invoke<SearchResults>("search_projects", { args }),
  curseforgeStatus: () => invoke<CurseForgeStatus>("curseforge_status"),
  getProject: (source: Source, id: string) => invoke<ProjectDetails>("get_project", { source, id }),
  getProjectMembers: (source: Source, id: string) => invoke<TeamMember[]>("get_project_members", { source, id }),
  getProjectVersions: (source: Source, id: string, loaders: string[] = [], gameVersions: string[] = []) =>
    invoke<ProjectVersion[]>("get_project_versions", { source, id, loaders, gameVersions }),
  getCategories: (source: Source) => invoke<Category[]>("get_categories", { source }),

  listContent: (id: string) => invoke<InstalledContent[]>("list_content", { id }),
  identifyContent: (id: string) => invoke<boolean>("identify_content", { id }),
  installContent: (id: string, kind: ContentKind, source: Source, projectId: string, versionId: string | null = null) =>
    invoke<void>("install_content", { id, kind, source, projectId, versionId }),
  checkContentFit: (kind: ContentKind, source: Source, projectId: string, versionId: string | null = null) =>
    invoke<Record<string, Fit>>("check_content_fit", { kind, source, projectId, versionId }),
  setContentEnabled: (id: string, kind: ContentKind, fileName: string, enabled: boolean) =>
    invoke<void>("set_content_enabled", { id, kind, fileName, enabled }),
  removeContent: (id: string, kind: ContentKind, fileName: string) =>
    invoke<void>("remove_content", { id, kind, fileName }),
  checkContentUpdates: (id: string) => invoke<ContentUpdate[]>("check_content_updates", { id }),
  installModpack: (source: Source, projectId: string, versionId: string | null = null) =>
    invoke<PackResult>("install_modpack", { source, projectId, versionId }),
  importPack: (path: string) => invoke<PackResult>("import_pack", { path }),
};

/** A local file as a URL the webview can load. */
export function fileUrl(path: string): string {
  return convertFileSrc(path);
}

/** Commands reject with plain strings; anything else is unexpected. */
export function errorMessage(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}

const CURSEFORGE_PATHS: Record<ProjectType, string> = {
  mod: "mc-mods",
  modpack: "modpacks",
  resourcepack: "texture-packs",
  shader: "shaders",
};

/** A project's page on its site. */
export function projectPageUrl(source: Source, type: ProjectType, slug: string): string {
  return source === "curseforge"
    ? `https://www.curseforge.com/minecraft/${CURSEFORGE_PATHS[type]}/${slug}`
    : `https://modrinth.com/${type}/${slug}`;
}
