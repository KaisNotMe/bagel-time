// Typed wrappers around the Rust commands in src-tauri/src/commands.rs.
import { invoke } from "@tauri-apps/api/core";

export type Loader = "vanilla" | "fabric" | "quilt";

export const LOADER_NAMES: Record<Loader, string> = {
  vanilla: "Vanilla",
  fabric: "Fabric",
  quilt: "Quilt",
};

export type LoaderVersion = { version: string; stable: boolean };

export type Instance = {
  id: string;
  name: string;
  gameVersion: string;
  loader: Loader;
  loaderVersion: string | null;
  memoryMb: number | null;
  /** Unix seconds. */
  created: number;
  lastPlayed: number | null;
  running: boolean;
};

export type VersionInfo = {
  id: string;
  kind: "release" | "snapshot";
  releaseTime: string;
};

export type Settings = {
  offlineUsername: string;
  memoryMb: number;
  showSnapshots: boolean;
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

export type ProjectType = "mod" | "modpack";
export type SortBy = "relevance" | "downloads" | "follows" | "newest" | "updated";

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
};

export type SearchResults = { hits: SearchHit[]; offset: number; limit: number; totalHits: number };

export type SearchArgs = {
  text: string;
  projectType: ProjectType;
  gameVersion: string | null;
  loader: Loader | null;
  sort: SortBy;
  offset: number;
  limit: number;
};

export type InstalledMod = {
  /** Without the .disabled suffix; identifies the mod in calls. */
  fileName: string;
  enabled: boolean;
  size: number;
  title: string;
  projectId: string | null;
  versionId: string | null;
  versionNumber: string | null;
  iconUrl: string | null;
  dependency: boolean;
};

export type ModUpdate = { fileName: string; projectId: string; versionId: string; versionNumber: string };

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
  deleteInstance: (id: string) => invoke<void>("delete_instance", { id }),
  openInstanceFolder: (id: string, mods = false) => invoke<void>("open_instance_folder", { id, mods }),
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),
  launchInstance: (id: string) => invoke<void>("launch_instance", { id }),
  stopInstance: (id: string) => invoke<boolean>("stop_instance", { id }),
  searchModrinth: (args: SearchArgs) => invoke<SearchResults>("search_modrinth", { args }),
  listMods: (id: string) => invoke<InstalledMod[]>("list_mods", { id }),
  identifyMods: (id: string) => invoke<boolean>("identify_mods", { id }),
  installMod: (id: string, projectId: string, versionId: string | null = null) =>
    invoke<InstalledMod[]>("install_mod", { id, projectId, versionId }),
  setModEnabled: (id: string, fileName: string, enabled: boolean) =>
    invoke<void>("set_mod_enabled", { id, fileName, enabled }),
  removeMod: (id: string, fileName: string) => invoke<void>("remove_mod", { id, fileName }),
  checkModUpdates: (id: string) => invoke<ModUpdate[]>("check_mod_updates", { id }),
  installModpack: (projectId: string, versionId: string | null = null) =>
    invoke<Instance>("install_modpack", { projectId, versionId }),
  importMrpack: (path: string) => invoke<Instance>("import_mrpack", { path }),
};

/** Commands reject with plain strings; anything else is unexpected. */
export function errorMessage(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}
