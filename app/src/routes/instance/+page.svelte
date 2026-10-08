<script lang="ts">
  import { page } from "$app/state";
  import {
    api,
    errorMessage,
    LOADER_NAMES,
    type Instance,
    type InstalledMod,
    type ModUpdate,
  } from "$lib/api";
  import { iconFor, relativeTime } from "$lib/format";
  import { games } from "$lib/games.svelte";
  import LogPanel from "$lib/components/LogPanel.svelte";
  import Modal from "$lib/components/Modal.svelte";
  import ModrinthBrowser from "$lib/components/ModrinthBrowser.svelte";
  import ProjectIcon from "$lib/components/ProjectIcon.svelte";

  let id = $derived(page.url.searchParams.get("id") ?? "");

  let instance = $state<Instance | null>(null);
  let loadError = $state("");
  let tab = $state<"installed" | "browse">("installed");

  let mods = $state<InstalledMod[]>([]);
  let modsLoaded = $state(false);
  let filter = $state("");
  let updates = $state<Record<string, ModUpdate>>({});
  let checking = $state(false);
  let checked = $state(false);
  /** fileName or projectId -> true while something runs for it. */
  let busy = $state<Record<string, boolean>>({});
  let actionError = $state("");
  let toRemove = $state<InstalledMod | null>(null);

  let status = $derived(instance ? games.status[instance.id] : undefined);
  let running = $derived(!!status);
  let moddable = $derived(instance !== null && instance.loader !== "vanilla");
  let installedProjects = $derived(new Set(mods.map((m) => m.projectId).filter(Boolean)));
  let updateCount = $derived(Object.keys(updates).length);
  let enabledCount = $derived(mods.filter((m) => m.enabled).length);
  let shown = $derived(
    filter.trim()
      ? mods.filter((m) =>
          `${m.title} ${m.fileName}`.toLowerCase().includes(filter.trim().toLowerCase()),
        )
      : mods,
  );
  let icon = $derived(iconFor(instance?.name ?? ""));

  async function loadInstance() {
    try {
      instance = await api.getInstance(id);
      games.syncRunning([instance]);
      loadError = "";
    } catch (e) {
      loadError = errorMessage(e);
    }
  }

  async function loadMods() {
    try {
      mods = await api.listMods(id);
    } catch (e) {
      actionError = errorMessage(e);
    } finally {
      modsLoaded = true;
    }
  }

  $effect(() => {
    if (!id) return;
    instance = null;
    mods = [];
    modsLoaded = false;
    updates = {};
    checked = false;
    tab = "installed";
    loadInstance();
    // Show the folder right away, then fill in names for hand-added jars.
    loadMods().then(async () => {
      try {
        if (await api.identifyMods(id)) await loadMods();
      } catch {
        // Offline: file names are good enough.
      }
    });
    return games.onChange(loadInstance);
  });

  async function guard(key: string, task: () => Promise<void>) {
    busy[key] = true;
    actionError = "";
    try {
      await task();
    } catch (e) {
      actionError = errorMessage(e);
    } finally {
      delete busy[key];
    }
  }

  function toggle(mod: InstalledMod) {
    return guard(mod.fileName, async () => {
      await api.setModEnabled(id, mod.fileName, !mod.enabled);
      mod.enabled = !mod.enabled;
    });
  }

  function remove(mod: InstalledMod) {
    toRemove = null;
    return guard(mod.fileName, async () => {
      await api.removeMod(id, mod.fileName);
      delete updates[mod.fileName];
      await loadMods();
    });
  }

  function install(projectId: string, versionId: string | null = null, key = projectId) {
    return guard(key, async () => {
      mods = await api.installMod(id, projectId, versionId);
    });
  }

  async function update(u: ModUpdate) {
    await install(u.projectId, u.versionId, u.fileName);
    if (!actionError) delete updates[u.fileName];
  }

  async function updateAll() {
    for (const u of Object.values(updates)) await update(u);
  }

  async function checkUpdates() {
    checking = true;
    actionError = "";
    try {
      const list = await api.checkModUpdates(id);
      updates = Object.fromEntries(list.map((u) => [u.fileName, u]));
      checked = true;
    } catch (e) {
      actionError = `Couldn't check for updates: ${errorMessage(e)}`;
    } finally {
      checking = false;
    }
  }
</script>

<div class="page" class:with-log={games.logOpenFor === id && !!instance}>
  <div class="scroll">
    <a class="back" href="/">
      <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
        <path d="M10 3 5 8l5 5" />
      </svg>
      Library
    </a>

    {#if loadError}
      <p class="alert">Couldn't open this instance: {loadError}</p>
    {:else if instance}
      <header>
        <div class="icon" style:background={icon.background}>{icon.initials}</div>
        <div class="info">
          <h1>{instance.name}</h1>
          <p class="meta">
            {LOADER_NAMES[instance.loader]}
            {instance.gameVersion}
            {#if instance.loaderVersion}<span class="faint">· loader {instance.loaderVersion}</span>{/if}
            <span class="faint">· {relativeTime(instance.lastPlayed)}</span>
          </p>
        </div>
        <div class="head-actions">
          <button class="btn" onclick={() => (games.logOpenFor = instance!.id)}>Game log</button>
          <button class="btn" onclick={() => api.openInstanceFolder(id, false)}>Open folder</button>
          {#if status?.phase === "running"}
            <button class="btn danger play" onclick={() => games.stop(id)}>Stop</button>
          {:else}
            <button class="btn primary play" disabled={!!status} onclick={() => games.launch(id)}>
              {status ? status.stage || "Starting…" : "Play"}
            </button>
          {/if}
        </div>
      </header>

      {#if games.errors[id]}
        <div class="alert row">
          <span>{games.errors[id]}</span>
          <button class="btn ghost" onclick={() => games.dismissError(id)}>Dismiss</button>
        </div>
      {/if}

      {#if !moddable}
        <div class="notice">
          <h2>Vanilla instance</h2>
          <p>
            Mods need a mod loader. Create a Fabric or Quilt instance to add mods, or install a modpack from the
            <a href="/modpacks">Modpacks</a> page.
          </p>
        </div>
      {:else}
        <div class="tabs" role="tablist">
          <button role="tab" aria-selected={tab === "installed"} class:active={tab === "installed"} onclick={() => (tab = "installed")}>
            Installed mods <span class="count">{mods.length}</span>
          </button>
          <button role="tab" aria-selected={tab === "browse"} class:active={tab === "browse"} onclick={() => (tab = "browse")}>
            Browse Modrinth
          </button>
        </div>

        {#if running}
          <p class="hint">Close the game to add, remove or switch off mods.</p>
        {/if}
        {#if actionError}
          <div class="alert row">
            <span>{actionError}</span>
            <button class="btn ghost" onclick={() => (actionError = "")}>Dismiss</button>
          </div>
        {/if}

        {#if tab === "installed"}
          <div class="toolbar">
            <input class="input grow" type="search" placeholder="Filter mods" bind:value={filter} aria-label="Filter mods" />
            {#if updateCount > 0}
              <button class="btn primary" disabled={running || Object.keys(busy).length > 0} onclick={updateAll}>
                Update all ({updateCount})
              </button>
            {:else}
              <button class="btn" disabled={checking || mods.length === 0} onclick={checkUpdates}>
                {checking ? "Checking…" : checked ? "Up to date ✓" : "Check for updates"}
              </button>
            {/if}
            <button class="btn" onclick={() => api.openInstanceFolder(id, true)}>Mods folder</button>
          </div>

          {#if modsLoaded && mods.length === 0}
            <div class="empty">
              <p>No mods yet.</p>
              <button class="btn primary" onclick={() => (tab = "browse")}>Browse Modrinth</button>
            </div>
          {:else}
            <p class="summary">{enabledCount} of {mods.length} enabled</p>
            <ul class="mods">
              {#each shown as mod (mod.fileName)}
                {@const upd = updates[mod.fileName]}
                <li class:off={!mod.enabled}>
                  <ProjectIcon url={mod.iconUrl} name={mod.title} size={40} />
                  <div class="mod-info">
                    <div class="mod-title">
                      <span class="name">{mod.title}</span>
                      {#if mod.dependency}<span class="tag">dependency</span>{/if}
                      {#if !mod.projectId}<span class="tag">not on Modrinth</span>{/if}
                    </div>
                    <p class="file" title={mod.fileName}>
                      {mod.versionNumber ?? mod.fileName}
                    </p>
                  </div>
                  {#if upd}
                    <button
                      class="btn small primary"
                      disabled={running || !!busy[mod.fileName]}
                      onclick={() => update(upd)}
                      title="Update to {upd.versionNumber}"
                    >
                      {busy[mod.fileName] ? "Updating…" : "Update"}
                    </button>
                  {/if}
                  <label class="switch" title={mod.enabled ? "Switch off" : "Switch on"}>
                    <input
                      type="checkbox"
                      checked={mod.enabled}
                      disabled={running || !!busy[mod.fileName]}
                      onchange={() => toggle(mod)}
                      aria-label="{mod.enabled ? 'Disable' : 'Enable'} {mod.title}"
                    />
                    <span></span>
                  </label>
                  <button
                    class="btn square ghost"
                    title="Remove"
                    aria-label="Remove {mod.title}"
                    disabled={running || !!busy[mod.fileName]}
                    onclick={() => (mod.projectId ? remove(mod) : (toRemove = mod))}
                  >
                    <svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
                      <path d="M3 4.5h10M6.5 4.5V3h3v1.5M4.5 4.5l.6 8.5h5.8l.6-8.5" />
                    </svg>
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        {:else}
          <ModrinthBrowser
            projectType="mod"
            gameVersion={instance.gameVersion}
            loader={instance.loader}
            placeholder="Search mods for {LOADER_NAMES[instance.loader]} {instance.gameVersion}"
          >
            {#snippet action(hit)}
              {#if installedProjects.has(hit.projectId)}
                <span class="installed">Installed ✓</span>
              {:else}
                <button
                  class="btn primary"
                  disabled={running || !!busy[hit.projectId]}
                  onclick={() => install(hit.projectId)}
                >
                  {busy[hit.projectId] ? "Installing…" : "Install"}
                </button>
              {/if}
            {/snippet}
          </ModrinthBrowser>
        {/if}
      {/if}
    {/if}
  </div>

  {#if games.logOpenFor === id && instance}
    <div class="log">
      <LogPanel instanceId={instance.id} name={instance.name} />
    </div>
  {/if}
</div>

<Modal open={toRemove !== null} title="Remove mod?" onclose={() => (toRemove = null)}>
  <p>
    <strong>{toRemove?.fileName}</strong> isn't on Modrinth, so Bagel Time can't download it again. Remove it anyway?
  </p>
  {#snippet footer()}
    <button class="btn ghost" onclick={() => (toRemove = null)}>Cancel</button>
    <button class="btn danger" onclick={() => toRemove && remove(toRemove)}>Remove</button>
  {/snippet}
</Modal>

<style>
  .page {
    display: grid;
    grid-template-rows: 1fr;
    height: 100%;
  }
  .page.with-log {
    grid-template-rows: 1fr minmax(200px, 38%);
  }
  .scroll {
    overflow: auto;
    padding: 20px 32px 40px;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .back {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    align-self: flex-start;
    color: var(--muted);
    text-decoration: none;
    font-weight: 600;
  }
  .back:hover {
    color: var(--text);
  }
  header {
    display: flex;
    align-items: center;
    gap: 16px;
  }
  .icon {
    flex: none;
    width: 64px;
    height: 64px;
    border-radius: 14px;
    display: grid;
    place-items: center;
    font-weight: 800;
    font-size: 23px;
    color: rgb(255 255 255 / 0.95);
    text-shadow: 0 1px 2px rgb(0 0 0 / 0.35);
  }
  .info {
    flex: 1;
    min-width: 0;
  }
  h1 {
    font-size: 24px;
    letter-spacing: -0.3px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta {
    color: var(--muted);
  }
  .faint {
    color: var(--faint);
  }
  .head-actions {
    display: flex;
    gap: 8px;
  }
  .play {
    min-width: 110px;
  }
  .notice {
    padding: 18px;
    border: 1px solid var(--line);
    border-radius: var(--radius);
    background: var(--card);
    display: grid;
    gap: 6px;
    color: var(--muted);
  }
  .notice h2 {
    font-size: 16px;
    color: var(--text);
  }
  .notice a {
    color: var(--accent);
  }
  .tabs {
    display: flex;
    gap: 4px;
    border-bottom: 1px solid var(--line);
  }
  .tabs button {
    padding: 9px 14px;
    border: 0;
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
    background: none;
    color: var(--muted);
    font-weight: 600;
    cursor: pointer;
  }
  .tabs button:hover {
    color: var(--text);
  }
  .tabs button.active {
    color: var(--text);
    border-bottom-color: var(--accent);
  }
  .count {
    margin-left: 4px;
    padding: 0 7px;
    border-radius: 999px;
    background: var(--card-hover);
    font-size: 12px;
  }
  .hint {
    color: var(--warn);
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  .row .btn {
    height: 28px;
    color: inherit;
  }
  .toolbar {
    display: flex;
    gap: 8px;
  }
  .grow {
    flex: 1;
  }
  .summary {
    color: var(--faint);
    font-size: 12.5px;
    margin-bottom: -8px;
  }
  .mods {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    border: 1px solid var(--line);
    border-radius: var(--radius);
    background: var(--card);
    overflow: hidden;
  }
  .mods li {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 9px 12px;
  }
  .mods li + li {
    border-top: 1px solid var(--line);
  }
  .mods li:hover {
    background: var(--card-hover);
  }
  .mods li.off .mod-info,
  .mods li.off :global(img),
  .mods li.off :global(.tile) {
    opacity: 0.45;
  }
  .mod-info {
    flex: 1;
    min-width: 0;
  }
  .mod-title {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .name {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tag {
    flex: none;
    padding: 0 7px;
    border-radius: 999px;
    border: 1px solid var(--line-strong);
    color: var(--faint);
    font-size: 11.5px;
  }
  .file {
    color: var(--faint);
    font-size: 12.5px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .small {
    height: 28px;
    padding: 0 10px;
    font-size: 13px;
  }
  .switch {
    position: relative;
    flex: none;
    width: 36px;
    height: 20px;
    cursor: pointer;
  }
  .switch input {
    position: absolute;
    opacity: 0;
    inset: 0;
    margin: 0;
    cursor: inherit;
  }
  .switch span {
    position: absolute;
    inset: 0;
    border-radius: 999px;
    background: var(--line-strong);
    transition: background 0.15s;
    pointer-events: none;
  }
  .switch span::after {
    content: "";
    position: absolute;
    top: 3px;
    left: 3px;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--text);
    transition: transform 0.15s;
  }
  .switch input:checked + span {
    background: var(--accent);
  }
  .switch input:checked + span::after {
    transform: translateX(16px);
    background: var(--accent-ink);
  }
  .switch input:disabled + span {
    opacity: 0.5;
  }
  .switch input:focus-visible + span {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .installed {
    color: var(--ok);
    font-weight: 600;
    padding: 0 6px;
  }
  .empty {
    display: grid;
    justify-items: center;
    gap: 10px;
    padding: 40px 0;
    color: var(--muted);
  }
  .log {
    min-height: 0;
  }
</style>
