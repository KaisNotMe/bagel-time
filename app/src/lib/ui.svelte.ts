// App-wide UI state: the top bar's breadcrumbs and the "new instance" dialog.

export type Crumb = { label: string; href?: string };

class Ui {
  crumbs = $state<Crumb[]>([]);
  creatingInstance = $state(false);

  /** Pages call this to say where the user is. */
  setCrumbs(...crumbs: Crumb[]) {
    this.crumbs = crumbs;
  }
}

export const ui = new Ui();
