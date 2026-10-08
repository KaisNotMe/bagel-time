// Live state of launching/running games, fed by events from src-tauri/src/games.rs.
import { listen } from "@tauri-apps/api/event";
import { api, errorMessage, type Instance, type LogLine } from "./api";

export type GameStatus = {
  phase: "preparing" | "running";
  stage: string;
  done: number;
  total: number;
};

const MAX_LOG_LINES = 3000;

class Games {
  status = $state<Record<string, GameStatus>>({});
  errors = $state<Record<string, string>>({});
  logs = $state.raw<Record<string, LogLine[]>>({});

  #stopping = new Set<string>();
  #pending = new Map<string, LogLine[]>();
  #flushQueued = false;
  #listeners = new Set<() => void>();

  /** Called when instances may have changed on disk (e.g. last played). */
  onChange(fn: () => void) {
    this.#listeners.add(fn);
    return () => this.#listeners.delete(fn);
  }

  /** After (re)loading the list, pick up games still running in the backend. */
  syncRunning(instances: Instance[]) {
    for (const i of instances) {
      if (i.running && !this.status[i.id]) {
        this.status[i.id] = { phase: "running", stage: "", done: 0, total: 0 };
      }
    }
  }

  async launch(id: string) {
    delete this.errors[id];
    this.logs = { ...this.logs, [id]: [] };
    this.status[id] = { phase: "preparing", stage: "Starting", done: 0, total: 0 };
    try {
      await api.launchInstance(id);
    } catch (e) {
      delete this.status[id];
      this.errors[id] = errorMessage(e);
    }
  }

  async stop(id: string) {
    this.#stopping.add(id);
    await api.stopInstance(id);
  }

  dismissError(id: string) {
    delete this.errors[id];
  }

  async listen() {
    await Promise.all([
      listen<{ instanceId: string; stage: string; done: number; total: number }>(
        "launch-progress",
        ({ payload: p }) => {
          this.status[p.instanceId] = { phase: "preparing", stage: p.stage, done: p.done, total: p.total };
        },
      ),
      listen<{ instanceId: string }>("game-started", ({ payload }) => {
        this.status[payload.instanceId] = { phase: "running", stage: "", done: 0, total: 0 };
        this.#changed();
      }),
      listen<{ instanceId: string; line: LogLine }>("game-log", ({ payload }) => {
        const queue = this.#pending.get(payload.instanceId) ?? [];
        queue.push(payload.line);
        this.#pending.set(payload.instanceId, queue);
        this.#queueFlush();
      }),
      listen<{ instanceId: string; code: number | null; error: string | null }>(
        "game-exited",
        ({ payload: p }) => {
          const stopped = this.#stopping.delete(p.instanceId);
          delete this.status[p.instanceId];
          if (p.error) {
            this.errors[p.instanceId] = p.error;
          } else if (!stopped && p.code !== null && p.code !== 0) {
            this.errors[p.instanceId] = `Minecraft closed unexpectedly (exit code ${p.code}). Check the game log for details.`;
          }
          this.#changed();
        },
      ),
    ]);
  }

  // Log lines arrive in bursts; apply them once per frame.
  #queueFlush() {
    if (this.#flushQueued) return;
    this.#flushQueued = true;
    requestAnimationFrame(() => {
      this.#flushQueued = false;
      const next = { ...this.logs };
      for (const [id, lines] of this.#pending) {
        next[id] = (next[id] ?? []).concat(lines).slice(-MAX_LOG_LINES);
      }
      this.#pending.clear();
      this.logs = next;
    });
  }

  #changed() {
    for (const fn of this.#listeners) fn();
  }
}

export const games = new Games();
