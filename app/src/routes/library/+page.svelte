<script lang="ts">
  import { goto } from "$app/navigation";
  import { api, errorMessage, LOADER_NAMES, type Instance } from "$lib/api";
  import BagelLogo from "$lib/components/BagelLogo.svelte";
  import Icon from "$lib/components/Icon.svelte";
  import InstanceCard from "$lib/components/InstanceCard.svelte";
  import InstanceSettings from "$lib/components/InstanceSettings.svelte";
  import Modal from "$lib/components/Modal.svelte";
  import { games } from "$lib/games.svelte";
  import { instances } from "$lib/instances.svelte";
  import { packs } from "$lib/packs.svelte";
  import { ui } from "$lib/ui.svelte";

  ui.setCrumbs({ label: "Library" });

  type Filter = "all" | "modded" | "vanilla";
  type Sort = "played" | "name" | "created" | "version";
  type Group = "none" | "loader" | "version";

  let search = $state("");
  let filter = $state<Filter>("all");
  let sort = $state<Sort>("played");
  let group = $state<Group>("none");

  let settingsFor = $state<Instance | null>(null);
  let toDelete = $state<Instance | null>(null);
  let deleting = $state(false);
  let actionError = $state("");

  let counts = $derived({
    all: instances.list.length,
    modded: instances.list.filter((i) => i.loader !== "vanilla").length,
    vanilla: instances.list.filter((i) => i.loader === "vanilla").length,
  });

  /** Newer Minecraft versions first, comparing number by number. */
  function compareVersions(a: string, b: string) {
    return b.localeCompare(a, undefined, { numeric: true });
  }

  let shown = $derived.by(() => {
    const q = search.trim().toLowerCase();
    let list = instances.list.filter(
      (i) =>
        (filter === "all" || (filter === "vanilla") === (i.loader === "vanilla")) &&
        (!q || `${i.name} ${i.gameVersion} ${i.loader}`.toLowerCase().includes(q)),
    );
    list = [...list];
    if (sort === "name") list.sort((a, b) => a.name.localeCompare(b.name));
    else if (sort === "created") list.sort((a, b) => b.created - a.created);
    else if (sort === "version") list.sort((a, b) => compareVersions(a.gameVersion, b.gameVersion));
    return list;
  });

  let groups = $derived.by(() => {
    if (group === "none") return [{ title: "", items: shown }];
    const map = new Map<string, Instance[]>();
    for (const i of shown) {
      const key = group === "loader" ? LOADER_NAMES[i.loader] : `Minecraft ${i.gameVersion}`;
      map.set(key, [...(map.get(key) ?? []), i]);
    }
    const entries = [...map.entries()];
    if (group === "version") entries.sort((a, b) => compareVersions(a[0], b[0]));
    else entries.sort((a, b) => a[0].localeCompare(b[0]));
    return entries.map(([title, items]) => ({ title, items }));
  });

  async function onmenu(instance: Instance, action: "folder" | "duplicate" | "settings" | "delete") {
    actionError = "";
    try {
      if (action === "folder") await api.openInstanceFolder(instance.id);
      else if (action === "settings") settingsFor = instance;
      else if (action === "delete") toDelete = instance;
      else {
        await api.duplicateInstance(instance.id, "");
        await instances.refresh();
      }
    } catch (e) {
      actionError = errorMessage(e);
    }
  }

  async function confirmDelete() {
    if (!toDelete) return;
    deleting = true;
    actionError = "";
    try {
      await api.deleteInstance(toDelete.id);
      toDelete = null;
      await instances.refresh();
    } catch (e) {
      actionError = errorMessage(e);
    } finally {
      deleting = false;
    }
  }

  // Keep the settings dialog showing the latest saved values.
  let settingsInstance = $derived(settingsFor ? (instances.get(settingsFor.id) ?? settingsFor) : null);
</script>

<div class="page">
  <div class="page-inner">
    <header class="head">
      <h1 class="page-title">Library</h1>
      <div class="head-actions">
        <button class="btn" disabled={packs.busy} onclick={() => packs.pickFile()}>
          <Icon name="upload" size={16} /> Import
        </button>
        <button class="btn primary" onclick={() => ui.newInstance()}>
          <Icon name="plus" size={16} /> New instance
        </button>
      </div>
    </header>

    <div class="toolbar">
      <label class="search-box">
        <Icon name="search" size={16} />
        <input class="input" type="search" placeholder="Search instances" bind:value={search} />
      </label>
      <label class="select">
        <span>Sort</span>
        <select class="input" bind:value={sort}>
          <option value="played">Last played</option>
          <option value="name">Name</option>
          <option value="created">Date created</option>
          <option value="version">Game version</option>
        </select>
      </label>
      <label class="select">
        <span>Group</span>
        <select class="input" bind:value={group}>
          <option value="none">None</option>
          <option value="loader">Loader</option>
          <option value="version">Game version</option>
        </select>
      </label>
    </div>

    <div class="pills" role="tablist">
      {#each [["all", "All instances"], ["modded", "Modded"], ["vanilla", "Vanilla"]] as [id, label] (id)}
        <button
          class="pill"
          class:active={filter === id}
          role="tab"
          aria-selected={filter === id}
          onclick={() => (filter = id as Filter)}
        >
          {label} <span class="count">{counts[id as Filter]}</span>
        </button>
      {/each}
    </div>

    {#if instances.error}<p class="alert">Couldn't read your instances: {instances.error}</p>{/if}
    {#if actionError}
      <div class="alert row">
        <span>{actionError}</span>
        <button class="btn ghost sm" onclick={() => (actionError = "")}>Dismiss</button>
      </div>
    {/if}

    {#if instances.loaded && instances.list.length === 0}
      <div class="empty-state">
        <BagelLogo size={72} />
        <h3>Nothing in the oven yet</h3>
        <p>Pick a Minecraft version or a modpack.</p>
        <div class="head-actions">
          <button class="btn primary" onclick={() => ui.newInstance()}>New instance</button>
          <a class="btn" href="/discover?type=modpack">Browse modpacks</a>
        </div>
      </div>
    {:else if shown.length === 0 && instances.loaded}
      <p class="empty-state">No instances match.</p>
    {:else}
      {#each groups as g (g.title)}
        {#if g.title}<h2 class="group-title">{g.title} <span class="faint">{g.items.length}</span></h2>{/if}
        <div class="grid">
          {#each g.items as instance (instance.id)}
            <InstanceCard {instance} onmenu={(a) => onmenu(instance, a)} />
          {/each}
        </div>
      {/each}
    {/if}
  </div>
</div>

{#if settingsInstance}
  <InstanceSettings
    open={settingsFor !== null}
    instance={settingsInstance}
    onclose={() => (settingsFor = null)}
    ondeleted={() => (settingsFor = null)}
    onduplicated={(copy) => {
      settingsFor = null;
      goto(`/instance?id=${encodeURIComponent(copy.id)}`);
    }}
  />
{/if}

<Modal open={toDelete !== null} title="Delete instance?" onclose={() => (toDelete = null)}>
  <p>
    <strong>{toDelete?.name}</strong> and its worlds will be gone for good.
  </p>
  {#snippet footer()}
    <button class="btn ghost" onclick={() => (toDelete = null)}>Cancel</button>
    <button
      class="btn danger"
      disabled={deleting || (toDelete !== null && !!games.status[toDelete.id])}
      onclick={confirmDelete}
    >
      {deleting ? "Deleting…" : "Delete forever"}
    </button>
  {/snippet}
</Modal>

<style>
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
  }
  .head-actions {
    display: flex;
    gap: 8px;
  }
  .toolbar {
    display: flex;
    gap: 10px;
    align-items: center;
  }
  .select {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--muted);
    font-weight: 600;
    font-size: 13px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(290px, 1fr));
    gap: 12px;
  }
  .group-title {
    font-size: 15px;
    margin-bottom: -8px;
  }
</style>
