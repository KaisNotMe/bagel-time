// App-wide UI state: the top bar's breadcrumbs and the "new instance" dialog.

import type { ContentKind, Source } from "./api";

export type Crumb = { label: string; href?: string };

/** A project to install into the instance being created. */
export type CreatingFor = {
  source: Source;
  projectId: string;
  title: string;
  kind: ContentKind;
  versionId: string | null;
};

class Ui {
  crumbs = $state<Crumb[]>([]);
  creatingInstance = $state(false);
  /** Set when the new instance is for a project: only fitting versions are offered. */
  creatingFor = $state<CreatingFor | null>(null);

  /** Pages call this to say where the user is. */
  setCrumbs(...crumbs: Crumb[]) {
    this.crumbs = crumbs;
  }

  /** Opens the "new instance" dialog, optionally set up for a project. */
  newInstance(forProject: CreatingFor | null = null) {
    this.creatingFor = forProject;
    this.creatingInstance = true;
  }

  closeNewInstance() {
    this.creatingInstance = false;
    this.creatingFor = null;
  }
}

export const ui = new Ui();
