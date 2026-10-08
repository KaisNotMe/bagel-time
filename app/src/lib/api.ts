// Typed wrappers around the Rust commands in src-tauri/src/commands.rs.
import { invoke } from "@tauri-apps/api/core";

export type Instance = {
  id: string;
  name: string;
  gameVersion: string;
  loader: "vanilla";
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

export const api = {
  listVersions: (snapshots: boolean) => invoke<VersionInfo[]>("list_versions", { snapshots }),
  listInstances: () => invoke<Instance[]>("list_instances"),
  createInstance: (name: string, gameVersion: string) =>
    invoke<Instance>("create_instance", { name, gameVersion }),
  deleteInstance: (id: string) => invoke<void>("delete_instance", { id }),
  openInstanceFolder: (id: string) => invoke<void>("open_instance_folder", { id }),
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),
  launchInstance: (id: string) => invoke<void>("launch_instance", { id }),
  stopInstance: (id: string) => invoke<boolean>("stop_instance", { id }),
};

/** Commands reject with plain strings; anything else is unexpected. */
export function errorMessage(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}
