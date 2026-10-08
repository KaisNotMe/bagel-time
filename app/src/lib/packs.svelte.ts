// Modpack installs (from Modrinth or a .mrpack file). One at a time; progress
// arrives through the `pack-progress` event from src-tauri/src/mods.rs.
import { goto } from "$app/navigation";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { api, errorMessage, type Instance } from "./api";

export type PackStatus = { name: string; stage: string; done: number; total: number };

class Packs {
  status = $state<PackStatus | null>(null);
  error = $state("");
  /** Project id being installed from Modrinth, if any. */
  installing = $state<string | null>(null);
  dragging = $state(false);

  #listeners = new Set<() => void>();

  /** Called after a pack becomes a new instance. */
  onInstalled(fn: () => void) {
    this.#listeners.add(fn);
    return () => this.#listeners.delete(fn);
  }

  get busy() {
    return this.status !== null;
  }

  async listen() {
    await listen<{ stage: string; done: number; total: number }>("pack-progress", ({ payload }) => {
      if (this.status) this.status = { ...this.status, ...payload };
    });
    await getCurrentWebview().onDragDropEvent(({ payload }) => {
      if (payload.type === "over" || payload.type === "enter") {
        this.dragging = true;
      } else if (payload.type === "leave") {
        this.dragging = false;
      } else if (payload.type === "drop") {
        this.dragging = false;
        const pack = payload.paths.find((p) => p.toLowerCase().endsWith(".mrpack"));
        if (pack) this.importFile(pack);
        else if (payload.paths.length) this.error = "Only Modrinth modpacks (.mrpack files) can be dropped here.";
      }
    });
  }

  async pickFile() {
    const path = await open({
      title: "Import a Modrinth modpack",
      multiple: false,
      directory: false,
      filters: [{ name: "Modrinth modpack", extensions: ["mrpack"] }],
    });
    if (typeof path === "string") await this.importFile(path);
  }

  importFile(path: string) {
    const name = path.split(/[\\/]/).pop() ?? "modpack";
    return this.#run(name, () => api.importMrpack(path));
  }

  async installFromModrinth(projectId: string, title: string) {
    this.installing = projectId;
    try {
      await this.#run(title, () => api.installModpack(projectId));
    } finally {
      this.installing = null;
    }
  }

  async #run(name: string, task: () => Promise<Instance>) {
    if (this.busy) {
      this.error = "Another modpack is still installing. Wait for it to finish first.";
      return;
    }
    this.error = "";
    this.status = { name, stage: "Starting", done: 0, total: 0 };
    try {
      const instance = await task();
      for (const fn of this.#listeners) fn();
      await goto(`/instance?id=${encodeURIComponent(instance.id)}`);
    } catch (e) {
      this.error = errorMessage(e);
    } finally {
      this.status = null;
    }
  }
}

export const packs = new Packs();
