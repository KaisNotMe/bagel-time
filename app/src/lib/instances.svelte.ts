// The instance list, shared by the sidebar, Home, Library and install pickers.
import { api, errorMessage, type Instance } from "./api";
import { games } from "./games.svelte";

class Instances {
  list = $state<Instance[]>([]);
  loaded = $state(false);
  error = $state("");
  /** Bumped when an icon changes so cached images reload. */
  iconVersion = $state<Record<string, number>>({});

  constructor() {
    games.onChange(() => this.refresh());
  }

  get(id: string): Instance | undefined {
    return this.list.find((i) => i.id === id);
  }

  /** Most recently played first (the backend's order). */
  get recent(): Instance[] {
    return this.list.slice(0, 5);
  }

  async refresh() {
    try {
      this.list = await api.listInstances();
      games.syncRunning(this.list);
      this.error = "";
    } catch (e) {
      this.error = errorMessage(e);
    } finally {
      this.loaded = true;
    }
  }

  /** Puts a changed instance into the list without a full reload. */
  replace(instance: Instance) {
    const i = this.list.findIndex((x) => x.id === instance.id);
    if (i >= 0) this.list[i] = instance;
    else this.list = [instance, ...this.list];
  }

  iconChanged(id: string) {
    this.iconVersion[id] = (this.iconVersion[id] ?? 0) + 1;
  }
}

export const instances = new Instances();
