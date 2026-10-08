<script lang="ts">
  import { api, errorMessage, fileUrl, type Instance, type Screenshot } from "$lib/api";
  import Icon from "../Icon.svelte";
  import Modal from "../Modal.svelte";

  let { instance }: { instance: Instance } = $props();

  let shots = $state<Screenshot[]>([]);
  let loaded = $state(false);
  let error = $state("");
  let viewing = $state<number | null>(null);
  let confirmDelete = $state(false);

  let current = $derived(viewing !== null ? shots[viewing] : null);

  async function load() {
    try {
      shots = await api.listScreenshots(instance.id);
    } catch (e) {
      error = errorMessage(e);
    } finally {
      loaded = true;
    }
  }

  $effect(() => {
    void instance.id;
    loaded = false;
    load();
  });

  function step(delta: number) {
    if (viewing === null || shots.length === 0) return;
    viewing = (viewing + delta + shots.length) % shots.length;
    confirmDelete = false;
  }

  async function remove() {
    if (!current) return;
    if (!confirmDelete) {
      confirmDelete = true;
      return;
    }
    try {
      await api.deleteScreenshot(instance.id, current.fileName);
      confirmDelete = false;
      const index = viewing!;
      await load();
      viewing = shots.length === 0 ? null : Math.min(index, shots.length - 1);
    } catch (e) {
      error = errorMessage(e);
    }
  }

  function taken(ms: number) {
    return new Date(ms).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
  }
</script>

<svelte:window
  onkeydown={(e) => {
    if (viewing === null) return;
    if (e.key === "ArrowRight") step(1);
    if (e.key === "ArrowLeft") step(-1);
  }}
/>

<div class="tab">
  <div class="toolbar">
    <span class="muted">{shots.length} {shots.length === 1 ? "screenshot" : "screenshots"} · press F2 in game to take one</span>
    <button class="btn" onclick={() => api.openInstanceFolder(instance.id, "screenshots")}>
      <Icon name="folder" size={16} /> Screenshots folder
    </button>
  </div>

  {#if error}<p class="alert">{error}</p>{/if}

  {#if loaded && shots.length === 0}
    <div class="empty-state card">
      <Icon name="camera" size={36} stroke={1.4} />
      <h3>No screenshots yet</h3>
      <p>Press F2 while playing to take one.</p>
    </div>
  {:else}
    <div class="grid">
      {#each shots as shot, i (shot.fileName)}
        <button class="shot" onclick={() => ((viewing = i), (confirmDelete = false))} aria-label="View {shot.fileName}">
          <img src={fileUrl(shot.path)} alt="" loading="lazy" />
          <span class="date">{taken(shot.modified)}</span>
        </button>
      {/each}
    </div>
  {/if}
</div>

<Modal open={current !== null} title={current ? taken(current.modified) : ""} onclose={() => (viewing = null)} width={1100}>
  {#if current}
    <div class="viewer">
      <button class="nav prev btn square" aria-label="Previous" onclick={() => step(-1)}><Icon name="chevron-left" /></button>
      <img src={fileUrl(current.path)} alt={current.fileName} />
      <button class="nav next btn square" aria-label="Next" onclick={() => step(1)}><Icon name="chevron-right" /></button>
    </div>
  {/if}
  {#snippet footer()}
    <span class="faint name">{current?.fileName}</span>
    <button class="btn {confirmDelete ? 'danger' : 'danger-soft'}" onclick={remove}>
      <Icon name="trash" size={15} />
      {confirmDelete ? "Click again to delete" : "Delete"}
    </button>
    <button class="btn" onclick={() => current && api.openScreenshot(instance.id, current.fileName)}>
      <Icon name="external" size={15} /> Open
    </button>
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
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(230px, 1fr));
    gap: 10px;
  }
  .shot {
    position: relative;
    padding: 0;
    border: 0;
    border-radius: var(--radius);
    overflow: hidden;
    background: var(--raised);
    aspect-ratio: 16 / 9;
    cursor: pointer;
  }
  .shot img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
    transition: transform 0.2s;
  }
  .shot:hover img {
    transform: scale(1.04);
  }
  .date {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    padding: 18px 10px 6px;
    background: linear-gradient(transparent, rgb(0 0 0 / 0.7));
    color: #fff;
    font-size: 12px;
    text-align: left;
    opacity: 0;
    transition: opacity 0.15s;
  }
  .shot:hover .date {
    opacity: 1;
  }
  .viewer {
    position: relative;
    display: grid;
    place-items: center;
  }
  .viewer img {
    max-width: 100%;
    max-height: calc(100vh - 220px);
    border-radius: var(--radius);
  }
  .nav {
    position: absolute;
    top: 50%;
    transform: translateY(-50%);
    background: rgb(0 0 0 / 0.55);
  }
  .prev {
    left: 8px;
  }
  .next {
    right: 8px;
  }
  .name {
    margin-right: auto;
    align-self: center;
    font-size: 13px;
  }
</style>
