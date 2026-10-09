<script lang="ts">
  import { api, errorMessage, fileUrl, type Instance, type World } from "$lib/api";
  import Icon from "../Icon.svelte";

  let { instance }: { instance: Instance } = $props();

  let worlds = $state<World[]>([]);
  let loaded = $state(false);
  let error = $state("");
  let search = $state("");

  let shown = $derived(
    search.trim() ? worlds.filter((w) => w.name.toLowerCase().includes(search.trim().toLowerCase())) : worlds,
  );

  $effect(() => {
    const id = instance.id;
    loaded = false;
    api
      .listWorlds(id)
      .then((w) => (worlds = w))
      .catch((e) => (error = errorMessage(e)))
      .finally(() => (loaded = true));
  });

  function played(ms: number | null) {
    if (!ms) return "Never played";
    const d = new Date(ms);
    return `Played ${d.toLocaleDateString(undefined, { dateStyle: "medium" })}, ${d.toLocaleTimeString([], { timeStyle: "short" })}`;
  }

  const MODES: Record<string, string> = {
    survival: "Survival",
    creative: "Creative",
    adventure: "Adventure",
    spectator: "Spectator",
  };
</script>

<div class="tab">
  <div class="toolbar">
    <label class="search-box">
      <Icon name="search" size={16} />
      <input class="input" type="search" placeholder="Search worlds" bind:value={search} />
    </label>
    <button class="btn" onclick={() => api.openInstanceFolder(instance.id, "saves")}>
      <Icon name="folder" size={16} /> Saves folder
    </button>
  </div>

  {#if error}<p class="alert">{error}</p>{/if}

  {#if loaded && worlds.length === 0}
    <div class="empty-state card">
      <Icon name="globe" size={36} stroke={1.4} />
      <h3>No worlds yet</h3>
    </div>
  {:else}
    <div class="list">
      {#each shown as world (world.folder)}
        <div class="world card">
          {#if world.icon}
            <img src={fileUrl(world.icon)} alt="" class="thumb" />
          {:else}
            <div class="thumb placeholder"><Icon name="globe" size={26} stroke={1.5} /></div>
          {/if}
          <div class="info">
            <h3>{world.name}</h3>
            <p class="tags">
              {#if world.hardcore}
                <span class="tag danger">Hardcore</span>
              {:else if world.gameMode}
                <span class="tag">{MODES[world.gameMode] ?? world.gameMode}</span>
              {/if}
              {#if world.version}<span class="tag">{world.version}</span>{/if}
            </p>
            <p class="faint small">{played(world.lastPlayed)}</p>
          </div>
          <button
            class="btn ghost square"
            title="Open world folder"
            aria-label="Open folder for {world.name}"
            onclick={() => api.openInstanceFolder(instance.id, `saves/${world.folder}`)}
          >
            <Icon name="folder" size={17} />
          </button>
        </div>
      {/each}
    </div>
  {/if}
</div>

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
  .list {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(340px, 1fr));
    gap: 10px;
  }
  .world {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 10px;
  }
  .world:hover {
    background: var(--raised-2);
  }
  .thumb {
    flex: none;
    width: 64px;
    height: 64px;
    border-radius: 10px;
    object-fit: cover;
    image-rendering: pixelated;
  }
  .placeholder {
    display: grid;
    place-items: center;
    background: var(--raised-3);
    color: var(--faint);
  }
  .info {
    flex: 1;
    min-width: 0;
    display: grid;
    gap: 4px;
  }
  h3 {
    font-size: 15px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tags {
    display: flex;
    gap: 4px;
  }
  .small {
    font-size: 12.5px;
  }
</style>
