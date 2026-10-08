<script lang="ts">
  import { onMount } from "svelte";
  import { api, errorMessage, type Instance } from "$lib/api";
  import BagelLogo from "$lib/components/BagelLogo.svelte";
  import InstanceCard from "$lib/components/InstanceCard.svelte";
  import LogPanel from "$lib/components/LogPanel.svelte";
  import Modal from "$lib/components/Modal.svelte";
  import NewInstanceDialog from "$lib/components/NewInstanceDialog.svelte";
  import { games } from "$lib/games.svelte";

  let instances = $state<Instance[]>([]);
  let loaded = $state(false);
  let loadError = $state("");
  let showSnapshots = $state(false);
  let creating = $state(false);
  let toDelete = $state<Instance | null>(null);
  let deleting = $state(false);
  let deleteError = $state("");

  let logInstance = $derived(instances.find((i) => i.id === games.logOpenFor));

  async function refresh() {
    try {
      instances = await api.listInstances();
      games.syncRunning(instances);
      loadError = "";
    } catch (e) {
      loadError = errorMessage(e);
    } finally {
      loaded = true;
    }
  }

  onMount(() => {
    refresh();
    api.getSettings().then((s) => (showSnapshots = s.showSnapshots));
    return games.onChange(refresh);
  });

  async function confirmDelete() {
    if (!toDelete) return;
    deleting = true;
    deleteError = "";
    try {
      await api.deleteInstance(toDelete.id);
      if (games.logOpenFor === toDelete.id) games.logOpenFor = null;
      toDelete = null;
      await refresh();
    } catch (e) {
      deleteError = errorMessage(e);
    } finally {
      deleting = false;
    }
  }

  async function openFolder(id: string, mods: boolean) {
    try {
      await api.openInstanceFolder(id, mods);
    } catch (e) {
      games.errors[id] = errorMessage(e);
    }
  }
</script>

<div class="page" class:with-log={!!logInstance}>
  <div class="scroll">
    <header>
      <div>
        <h1>Library</h1>
        <p class="sub">
          {instances.length}
          {instances.length === 1 ? "instance" : "instances"}
        </p>
      </div>
      <button class="btn primary" onclick={() => (creating = true)}>
        <svg width="14" height="14" viewBox="0 0 14 14" stroke="currentColor" stroke-width="2" stroke-linecap="round">
          <path d="M7 2v10M2 7h10" />
        </svg>
        New instance
      </button>
    </header>

    {#if loadError}
      <p class="alert">Couldn't read your instances: {loadError}</p>
    {/if}

    {#if loaded && instances.length === 0 && !loadError}
      <div class="empty">
        <BagelLogo size={72} />
        <h2>Nothing in the oven yet</h2>
        <p>Create an instance to pick a Minecraft version and start playing.</p>
        <button class="btn primary" onclick={() => (creating = true)}>Create your first instance</button>
      </div>
    {:else}
      <div class="grid">
        {#each instances as instance (instance.id)}
          <InstanceCard
            {instance}
            ondelete={() => ((deleteError = ""), (toDelete = instance))}
            onopenfolder={(mods) => openFolder(instance.id, mods)}
          />
        {/each}
      </div>
    {/if}
  </div>

  {#if logInstance}
    <div class="log">
      <LogPanel instanceId={logInstance.id} name={logInstance.name} />
    </div>
  {/if}
</div>

<NewInstanceDialog
  open={creating}
  {showSnapshots}
  onclose={() => (creating = false)}
  oncreated={async () => {
    creating = false;
    await refresh();
  }}
/>

<Modal open={toDelete !== null} title="Delete instance?" onclose={() => (toDelete = null)}>
  <p>
    <strong>{toDelete?.name}</strong> and everything in its folder will be permanently deleted, including
    its worlds and screenshots. This can't be undone.
  </p>
  {#if deleteError}<p class="alert">{deleteError}</p>{/if}
  {#snippet footer()}
    <button class="btn ghost" onclick={() => (toDelete = null)}>Cancel</button>
    <button class="btn danger" disabled={deleting} onclick={confirmDelete}>
      {deleting ? "Deleting…" : "Delete forever"}
    </button>
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
    padding: 28px 32px 40px;
    min-height: 0;
  }
  header {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 22px;
  }
  h1 {
    font-size: 26px;
    letter-spacing: -0.3px;
  }
  .sub {
    color: var(--muted);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(250px, 1fr));
    gap: 14px;
  }
  .empty {
    display: grid;
    justify-items: center;
    gap: 10px;
    margin-top: 12vh;
    text-align: center;
    color: var(--muted);
  }
  .empty h2 {
    color: var(--text);
    font-size: 19px;
    margin-top: 8px;
  }
  .empty .btn {
    margin-top: 10px;
  }
  .log {
    min-height: 0;
  }
  .alert {
    margin-bottom: 16px;
  }
</style>
