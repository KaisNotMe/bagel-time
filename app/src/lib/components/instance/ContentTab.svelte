<script lang="ts">
  import {
    api,
    errorMessage,
    PROJECT_TYPE_NAMES,
    supportedKinds,
    type ContentKind,
    type ContentUpdate,
    type Instance,
    type InstalledContent,
  } from "$lib/api";
  import Icon from "../Icon.svelte";
  import Modal from "../Modal.svelte";
  import ProjectIcon from "../ProjectIcon.svelte";

  type Props = { instance: Instance; running: boolean };
  let { instance, running }: Props = $props();

  const FOLDERS: Record<ContentKind, string> = { mod: "mods", resourcepack: "resourcepacks", shader: "shaderpacks" };

  let items = $state<InstalledContent[]>([]);
  let loaded = $state(false);
  let kindFilter = $state<ContentKind | "all">("all");
  let search = $state("");
  let updates = $state<Record<string, ContentUpdate>>({});
  let checking = $state(false);
  let checked = $state(false);
  let selected = $state<Record<string, boolean>>({});
  let busy = $state<Record<string, boolean>>({});
  let error = $state("");
  let confirmRemove = $state<InstalledContent[] | null>(null);

  const key = (c: { kind: ContentKind; fileName: string }) => `${c.kind}:${c.fileName}`;

  let kinds = $derived(supportedKinds(instance.loader));
  let counts = $derived(
    Object.fromEntries(kinds.map((k) => [k, items.filter((i) => i.kind === k).length])) as Record<ContentKind, number>,
  );
  let shown = $derived.by(() => {
    const q = search.trim().toLowerCase();
    return items.filter(
      (i) =>
        (kindFilter === "all" || i.kind === kindFilter) &&
        (!q || `${i.title} ${i.fileName}`.toLowerCase().includes(q)),
    );
  });
  let selectedItems = $derived(items.filter((i) => selected[key(i)]));
  let allShownSelected = $derived(shown.length > 0 && shown.every((i) => selected[key(i)]));
  let updateList = $derived(Object.values(updates));
  let anyBusy = $derived(Object.keys(busy).length > 0);
  let needsIris = $derived(
    counts.shader > 0 && !items.some((i) => i.kind === "mod" && /\b(iris|oculus)\b/i.test(i.title)),
  );
  let addKind = $derived<ContentKind>(kindFilter === "all" ? kinds[0] : kindFilter);

  async function load() {
    try {
      items = await api.listContent(instance.id);
    } catch (e) {
      error = errorMessage(e);
    } finally {
      loaded = true;
    }
  }

  // Reload when switching instances; then look up hand-added files.
  $effect(() => {
    void instance.id;
    void instance.loader;
    items = [];
    loaded = false;
    updates = {};
    checked = false;
    selected = {};
    kindFilter = "all";
    load().then(async () => {
      try {
        if (await api.identifyContent(instance.id)) await load();
      } catch {
        // Offline: file names are good enough.
      }
    });
  });

  async function guard(keys: string[], task: () => Promise<void>) {
    for (const k of keys) busy[k] = true;
    error = "";
    try {
      await task();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      for (const k of keys) delete busy[k];
    }
  }

  function setEnabled(list: InstalledContent[], enabled: boolean) {
    const targets = list.filter((i) => i.enabled !== enabled);
    return guard(targets.map(key), async () => {
      for (const i of targets) {
        await api.setContentEnabled(instance.id, i.kind, i.fileName, enabled);
        i.enabled = enabled;
      }
    });
  }

  function remove(list: InstalledContent[]) {
    confirmRemove = null;
    return guard(list.map(key), async () => {
      for (const i of list) {
        await api.removeContent(instance.id, i.kind, i.fileName);
        delete updates[key(i)];
        delete selected[key(i)];
      }
      await load();
    });
  }

  function askRemove(list: InstalledContent[]) {
    // Files from Modrinth can be downloaded again; anything else gets a check first.
    if (list.length === 1 && list[0].projectId) remove(list);
    else confirmRemove = list;
  }

  function update(list: ContentUpdate[]) {
    return guard(list.map(key), async () => {
      for (const u of list) {
        await api.installContent(instance.id, u.kind, u.projectId, u.versionId);
        delete updates[key(u)];
      }
      await load();
    });
  }

  async function checkUpdates() {
    checking = true;
    error = "";
    try {
      const list = await api.checkContentUpdates(instance.id);
      updates = Object.fromEntries(list.map((u) => [key(u), u]));
      checked = true;
    } catch (e) {
      error = `Couldn't check for updates: ${errorMessage(e)}`;
    } finally {
      checking = false;
    }
  }

  function toggleAll() {
    const value = !allShownSelected;
    for (const i of shown) selected[key(i)] = value;
  }

  // Forge runs shader packs through Oculus; everything else through Iris.
  let shaderMod = $derived(
    instance.loader === "forge" ? { slug: "oculus", name: "Oculus" } : { slug: "iris", name: "Iris" },
  );

  function installShaderMod() {
    return guard(["iris"], async () => {
      await api.installContent(instance.id, "mod", shaderMod.slug);
      await load();
    });
  }
</script>

<div class="tab">
  <div class="toolbar">
    <label class="search-box">
      <Icon name="search" size={16} />
      <input class="input" type="search" placeholder="Search {items.length} files" bind:value={search} />
    </label>
    {#if updateList.length > 0}
      <button class="btn primary" disabled={running || anyBusy} onclick={() => update(updateList)}>
        <Icon name="download" size={16} /> Update all ({updateList.length})
      </button>
    {:else}
      <button class="btn" disabled={checking || items.length === 0} onclick={checkUpdates}>
        <Icon name={checked ? "check" : "refresh"} size={16} />
        {checking ? "Checking…" : checked ? "Up to date" : "Check for updates"}
      </button>
    {/if}
    <button
      class="btn ghost square"
      title="Open {FOLDERS[addKind]} folder"
      aria-label="Open folder"
      onclick={() => api.openInstanceFolder(instance.id, FOLDERS[addKind])}
    >
      <Icon name="folder" size={17} />
    </button>
    <a class="btn primary" href="/discover?type={addKind}&instance={encodeURIComponent(instance.id)}">
      <Icon name="plus" size={16} /> Add content
    </a>
  </div>

  <div class="pills">
    <button class="pill" class:active={kindFilter === "all"} onclick={() => (kindFilter = "all")}>
      All <span class="count">{items.length}</span>
    </button>
    {#each kinds as k (k)}
      <button class="pill" class:active={kindFilter === k} onclick={() => (kindFilter = k)}>
        {PROJECT_TYPE_NAMES[k].many} <span class="count">{counts[k]}</span>
      </button>
    {/each}
  </div>

  {#if instance.loader === "vanilla"}
    <p class="hint"><Icon name="info" size={15} /> Mods and shaders need Fabric or Quilt. Vanilla instances can use resource packs.</p>
  {/if}
  {#if needsIris}
    <div class="hint warn-box">
      <Icon name="sun" size={16} />
      <span>Shader packs need the {shaderMod.name} mod to work.</span>
      <button class="btn sm primary" disabled={running || !!busy.iris} onclick={installShaderMod}>
        {busy.iris ? "Installing…" : `Install ${shaderMod.name}`}
      </button>
    </div>
  {/if}
  {#if running}<p class="hint warn"><Icon name="info" size={15} /> Close the game to change its content.</p>{/if}
  {#if error}
    <div class="alert row">
      <span>{error}</span>
      <button class="btn ghost sm" onclick={() => (error = "")}>Dismiss</button>
    </div>
  {/if}

  {#if loaded && items.length === 0}
    <div class="empty-state card">
      <Icon name="puzzle" size={36} stroke={1.4} />
      <h3>No content yet</h3>
      <p>Add mods, resource packs or shaders from Modrinth.</p>
      <a class="btn primary" href="/discover?type={kinds[0]}&instance={encodeURIComponent(instance.id)}">
        <Icon name="compass" size={16} /> Discover content
      </a>
    </div>
  {:else if loaded}
    <div class="table card">
      <div class="thead">
        <label class="cb" title="Select all">
          <input type="checkbox" checked={allShownSelected} onchange={toggleAll} aria-label="Select all" />
        </label>
        {#if selectedItems.length > 0}
          <div class="bulk">
            <b>{selectedItems.length} selected</b>
            <button class="btn sm" disabled={running || anyBusy} onclick={() => setEnabled(selectedItems, true)}>Enable</button>
            <button class="btn sm" disabled={running || anyBusy} onclick={() => setEnabled(selectedItems, false)}>Disable</button>
            {#if selectedItems.some((i) => updates[key(i)])}
              <button
                class="btn sm"
                disabled={running || anyBusy}
                onclick={() => update(selectedItems.map((i) => updates[key(i)]).filter(Boolean))}>Update</button
              >
            {/if}
            <button class="btn sm danger-soft" disabled={running || anyBusy} onclick={() => askRemove(selectedItems)}>
              Remove
            </button>
            <button class="btn sm ghost" onclick={() => (selected = {})}>Clear</button>
          </div>
        {:else}
          <span class="col-name">Name</span>
          <span class="col-version">Version</span>
          <span class="col-actions">Actions</span>
        {/if}
      </div>

      {#each shown as item (key(item))}
        {@const k = key(item)}
        {@const upd = updates[k]}
        <div class="tr" class:off={!item.enabled} class:sel={selected[k]}>
          <label class="cb">
            <input type="checkbox" bind:checked={selected[k]} aria-label="Select {item.title}" />
          </label>
          <div class="col-name name-cell">
            <ProjectIcon url={item.iconUrl} name={item.title} size={38} />
            <div class="name-text">
              {#if item.projectId}
                <a class="title" href="/project?id={item.projectId}&instance={encodeURIComponent(instance.id)}">{item.title}</a>
              {:else}
                <span class="title">{item.title}</span>
              {/if}
              <span class="tags">
                {#if kindFilter === "all"}<span class="tag">{PROJECT_TYPE_NAMES[item.kind].one}</span>{/if}
                {#if item.dependency}<span class="tag">Dependency</span>{/if}
                {#if !item.projectId}<span class="tag">Not on Modrinth</span>{/if}
              </span>
            </div>
          </div>
          <div class="col-version version-cell">
            <span class="ver">{item.versionNumber ?? "—"}</span>
            <span class="file" title={item.fileName}>{item.fileName}</span>
          </div>
          <div class="col-actions actions">
            {#if upd}
              <button
                class="btn sm primary"
                disabled={running || !!busy[k]}
                title="Update to {upd.versionNumber}"
                onclick={() => update([upd])}
              >
                <Icon name="download" size={14} />
                {busy[k] ? "Updating…" : "Update"}
              </button>
            {/if}
            <label class="switch" title={item.enabled ? "Disable" : "Enable"}>
              <input
                type="checkbox"
                checked={item.enabled}
                disabled={running || !!busy[k]}
                onchange={() => setEnabled([item], !item.enabled)}
                aria-label="{item.enabled ? 'Disable' : 'Enable'} {item.title}"
              />
              <span></span>
            </label>
            <button
              class="btn ghost sm square"
              title="Remove"
              aria-label="Remove {item.title}"
              disabled={running || !!busy[k]}
              onclick={() => askRemove([item])}
            >
              <Icon name="trash" size={16} />
            </button>
          </div>
        </div>
      {:else}
        <p class="none">Nothing matches.</p>
      {/each}
    </div>
  {/if}
</div>

<Modal open={confirmRemove !== null} title="Remove {confirmRemove?.length === 1 ? 'file' : `${confirmRemove?.length} files`}?" onclose={() => (confirmRemove = null)}>
  {#if confirmRemove?.some((i) => !i.projectId)}
    <p>Some of these aren't on Modrinth, so Bagel Time can't download them again.</p>
  {/if}
  <ul class="remove-list">
    {#each confirmRemove ?? [] as i (key(i))}<li>{i.title}</li>{/each}
  </ul>
  {#snippet footer()}
    <button class="btn ghost" onclick={() => (confirmRemove = null)}>Cancel</button>
    <button class="btn danger" onclick={() => confirmRemove && remove(confirmRemove)}>Remove</button>
  {/snippet}
</Modal>

<style>
  .tab {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .toolbar {
    display: flex;
    gap: 8px;
  }
  .hint {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--muted);
    font-size: 13px;
  }
  .hint.warn {
    color: var(--warn);
  }
  .warn-box {
    padding: 8px 8px 8px 12px;
    border-radius: 10px;
    background: rgb(233 196 106 / 0.1);
    color: var(--warn);
  }
  .warn-box span {
    flex: 1;
  }
  .table {
    overflow: hidden;
  }
  .thead,
  .tr {
    display: grid;
    grid-template-columns: 40px minmax(0, 1.4fr) minmax(0, 1fr) auto;
    align-items: center;
    gap: 12px;
    padding: 0 14px 0 8px;
  }
  .thead {
    height: 46px;
    border-bottom: 1px solid var(--line);
    color: var(--faint);
    font-size: 12px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.4px;
  }
  .bulk {
    grid-column: 2 / -1;
    display: flex;
    align-items: center;
    gap: 6px;
    text-transform: none;
    letter-spacing: 0;
    font-size: 13px;
    color: var(--text);
  }
  .bulk b {
    margin-right: 6px;
  }
  .col-actions {
    justify-self: end;
  }
  .tr {
    min-height: 60px;
    padding-top: 8px;
    padding-bottom: 8px;
  }
  .tr + .tr {
    border-top: 1px solid var(--line);
  }
  .tr:hover {
    background: var(--raised-2);
  }
  .tr.sel {
    background: var(--accent-soft);
  }
  .tr.off .name-cell,
  .tr.off .version-cell {
    opacity: 0.45;
  }
  .cb {
    display: grid;
    place-items: center;
    height: 100%;
    cursor: pointer;
  }
  .cb input {
    width: 16px;
    height: 16px;
    accent-color: var(--accent);
    cursor: pointer;
  }
  .name-cell {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
  }
  .name-text {
    display: grid;
    gap: 3px;
    min-width: 0;
  }
  .title {
    font-weight: 600;
    color: var(--text);
    text-decoration: none;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  a.title:hover {
    color: var(--accent);
  }
  .tags {
    display: flex;
    gap: 4px;
  }
  .tags:empty {
    display: none;
  }
  .version-cell {
    display: grid;
    min-width: 0;
  }
  .ver,
  .file {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ver {
    font-weight: 600;
    font-size: 13px;
  }
  .file {
    color: var(--faint);
    font-size: 12px;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .none {
    padding: 24px;
    text-align: center;
    color: var(--faint);
  }
  .remove-list {
    margin: 0;
    padding-left: 18px;
    max-height: 200px;
    overflow: auto;
    color: var(--muted);
  }
</style>
