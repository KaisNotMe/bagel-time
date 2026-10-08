// Server status (pinged on demand, cached briefly) and recently played servers.
import { listen } from "@tauri-apps/api/event";
import { api, errorMessage, type RecentServer, type ServerStatus } from "./api";

export type Ping =
  | { state: "loading" }
  | { state: "online"; status: ServerStatus }
  | { state: "offline"; error: string };

/** Pings newer than this are reused. */
const FRESH_MS = 30_000;

class Servers {
  pings = $state<Record<string, Ping>>({});
  recent = $state<RecentServer[]>([]);
  #pingedAt = new Map<string, number>();

  /** Pings a server unless it was pinged recently (or `force`). */
  async ping(address: string, force = false) {
    const key = address.toLowerCase();
    const at = this.#pingedAt.get(key);
    if (!force && at && Date.now() - at < FRESH_MS) return;
    this.#pingedAt.set(key, Date.now());
    this.pings[key] = { state: "loading" };
    try {
      this.pings[key] = { state: "online", status: await api.pingServer(address) };
    } catch (e) {
      this.pings[key] = { state: "offline", error: errorMessage(e) };
    }
  }

  get(address: string): Ping | undefined {
    return this.pings[address.toLowerCase()];
  }

  async loadRecent() {
    try {
      this.recent = await api.recentServers();
    } catch {
      // Not important enough to show.
    }
  }

  async listen() {
    await listen("servers-changed", () => this.loadRecent());
  }
}

export const servers = new Servers();
