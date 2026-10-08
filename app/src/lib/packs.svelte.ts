// Modpack installs (from Modrinth, CurseForge, or a .mrpack / CurseForge .zip
// file). One at a time; progress arrives through the `pack-progress` event
// from src-tauri/src/content.rs.
import { goto } from "$app/navigation";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { api, errorMessage, type Instance, type ManualDownload, type PackResult, type Source } from "./api";
import { instances } from "./instances.svelte";

export type PackStatus = { name: string; stage: string; done: number; total: number };

const PACK_FILE = /\.(mrpack|zip)$/i;

class Packs {
  status = $state<PackStatus | null>(null);
  error = $state("");
  /** Project id being installed from a site, if any. */
  installing = $state<string | null>(null);
  dragging = $state(false);
  /** Files from the last install that the author only offers on the website. */
  manual = $state<{ instance: Instance; files: ManualDownload[] } | null>(null);

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
        const pack = payload.paths.find((p) => PACK_FILE.test(p));
        if (pack) this.importFile(pack);
        else if (payload.paths.length) this.error = "Only modpacks (.mrpack or CurseForge .zip files) can be dropped here.";
      }
    });
  }

  async pickFile() {
    const path = await open({
      title: "Import a modpack",
      multiple: false,
      directory: false,
      filters: [{ name: "Modpack (.mrpack, CurseForge .zip)", extensions: ["mrpack", "zip"] }],
    });
    if (typeof path === "string") await this.importFile(path);
  }

  importFile(path: string) {
    const name = path.split(/[\\/]/).pop() ?? "modpack";
    return this.#run(name, () => api.importPack(path));
  }

  async install(source: Source, projectId: string, title: string, versionId: string | null = null) {
    this.installing = projectId;
    try {
      await this.#run(title, () => api.installModpack(source, projectId, versionId));
    } finally {
      this.installing = null;
    }
  }

  async #run(name: string, task: () => Promise<PackResult>) {
    if (this.busy) {
      this.error = "Another modpack is still installing. Wait for it to finish first.";
      return;
    }
    this.error = "";
    this.status = { name, stage: "Starting", done: 0, total: 0 };
    try {
      const { instance, manual } = await task();
      await instances.refresh();
      if (manual.length) this.manual = { instance, files: manual };
      await goto(`/instance?id=${encodeURIComponent(instance.id)}`);
    } catch (e) {
      this.error = errorMessage(e);
    } finally {
      this.status = null;
    }
  }
}

export const packs = new Packs();
