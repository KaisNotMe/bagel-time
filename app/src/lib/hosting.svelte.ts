// Servers the player hosts, fed by events from src-tauri/src/hosting.rs.
import { listen } from "@tauri-apps/api/event";
import { api, errorMessage, type HostStatus, type HostedServer, type LogLine } from "./api";

const MAX_LINES = 2000;

class Hosting {
  list = $state<HostedServer[]>([]);
  loaded = $state(false);
  consoles = $state.raw<Record<string, LogLine[]>>({});
  /** Errors from starting or stopping, shown on the server page. */
  errors = $state<Record<string, string>>({});
  /** Client-only mods left out when a server was made, shown once. */
  skipped = $state<Record<string, string[]>>({});
  playitConnected = $state<boolean | null>(null);
  /** The "host a server" dialog; an instance id preselects it. */
  dialog = $state<{ open: boolean; fromInstance: string | null }>({ open: false, fromInstance: null });

  #pending = new Map<string, LogLine[]>();
  #flushQueued = false;

  get(id: string): HostedServer | undefined {
    return this.list.find((s) => s.id === id);
  }

  async refresh() {
    try {
      this.list = await api.listHostedServers();
    } catch {
      // Keep the last list.
    } finally {
      this.loaded = true;
    }
  }

  async checkPlayit() {
    try {
      this.playitConnected = (await api.playitStatus()).connected;
    } catch {
      this.playitConnected = false;
    }
  }

  open(fromInstance: string | null = null) {
    this.dialog = { open: true, fromInstance };
  }

  async loadConsole(id: string) {
    try {
      const lines = await api.serverConsole(id);
      this.consoles = { ...this.consoles, [id]: lines };
    } catch {
      // Not important enough to show.
    }
  }

  async start(id: string) {
    delete this.errors[id];
    this.consoles = { ...this.consoles, [id]: [] };
    try {
      await api.startHostedServer(id);
    } catch (e) {
      this.errors[id] = errorMessage(e);
    }
  }

  async stop(id: string) {
    try {
      await api.stopHostedServer(id);
    } catch (e) {
      this.errors[id] = errorMessage(e);
    }
  }

  #setStatus(id: string, status: HostStatus) {
    const server = this.get(id);
    if (!server) {
      this.refresh();
      return;
    }
    server.status = status;
    if (status.publicAddress) server.publicAddress = status.publicAddress;
    if (status.error) this.errors[id] = status.error;
  }

  async listen() {
    await Promise.all([
      listen<{ serverId: string; status: HostStatus }>("host-status", ({ payload }) =>
        this.#setStatus(payload.serverId, payload.status),
      ),
      listen<{ serverId: string; line: LogLine }>("host-log", ({ payload }) => {
        const queue = this.#pending.get(payload.serverId) ?? [];
        queue.push(payload.line);
        this.#pending.set(payload.serverId, queue);
        this.#queueFlush();
      }),
    ]);
  }

  // Busy servers print a lot; apply lines once per frame.
  #queueFlush() {
    if (this.#flushQueued) return;
    this.#flushQueued = true;
    requestAnimationFrame(() => {
      this.#flushQueued = false;
      const next = { ...this.consoles };
      for (const [id, lines] of this.#pending) {
        const merged = [...(next[id] ?? []), ...lines];
        next[id] = merged.length > MAX_LINES ? merged.slice(-MAX_LINES) : merged;
      }
      this.#pending.clear();
      this.consoles = next;
    });
  }
}

export const hosting = new Hosting();
