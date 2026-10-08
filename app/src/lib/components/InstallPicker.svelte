<script lang="ts">
  import { api, errorMessage, LOADER_NAMES, PROJECT_TYPE_NAMES, type ContentKind, type Source } from "$lib/api";
  import { games } from "$lib/games.svelte";
  import { instances } from "$lib/instances.svelte";
  import { ui } from "$lib/ui.svelte";
  import Icon from "./Icon.svelte";
  import InstanceIcon from "./InstanceIcon.svelte";
  import Modal from "./Modal.svelte";

  type Props = {
    open: boolean;
    source: Source;
    projectId: string;
    title: string;
    kind: ContentKind;
    versionId?: string | null;
    onclose: () => void;
  };
  let { open, source, projectId, title, kind, versionId = null, onclose }: Props = $props();

  type RowState = { busy?: boolean; done?: boolean; error?: string };
  let rows = $state<Record<string, RowState>>({});

  $effect(() => {
    if (open) rows = {};
  });

  // Vanilla instances only take resource packs.
  let compatible = $derived(instances.list.filter((i) => kind === "resourcepack" || i.loader !== "vanilla"));

  async function install(id: string) {
    rows[id] = { busy: true };
    try {
      await api.installContent(id, kind, source, projectId, versionId);
      rows[id] = { done: true };
    } catch (e) {
      rows[id] = { error: errorMessage(e) };
    }
  }
</script>

<Modal {open} title="Install {title}" {onclose} width={520}>
  <p class="muted">Choose an instance to add this {PROJECT_TYPE_NAMES[kind].one.toLowerCase()} to.</p>
  <div class="list">
    {#each compatible as inst (inst.id)}
      {@const row = rows[inst.id] ?? {}}
      <div class="row">
        <InstanceIcon instance={inst} size={40} />
        <div class="info">
          <b>{inst.name}</b>
          <small>{LOADER_NAMES[inst.loader]} {inst.gameVersion}</small>
          {#if row.error}<small class="err">{row.error}</small>{/if}
        </div>
        {#if row.done}
          <span class="tag ok"><Icon name="check" size={13} /> Installed</span>
        {:else}
          <button
            class="btn sm primary"
            disabled={row.busy || !!games.status[inst.id]}
            title={games.status[inst.id] ? "Close the game first" : undefined}
            onclick={() => install(inst.id)}
          >
            {row.busy ? "Installing…" : "Install"}
          </button>
        {/if}
      </div>
    {:else}
      <p class="empty">
        {kind === "resourcepack" ? "You don't have any instances yet." : "You need a modded instance (Fabric, Quilt, Forge or NeoForge) for this."}
      </p>
    {/each}
  </div>
  {#snippet footer()}
    <button
      class="btn ghost new"
      onclick={() => {
        onclose();
        ui.creatingInstance = true;
      }}><Icon name="plus" size={15} /> New instance</button
    >
    <button class="btn" onclick={onclose}>Done</button>
  {/snippet}
</Modal>

<style>
  .list {
    display: grid;
    gap: 4px;
    max-height: 50vh;
    overflow: auto;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px;
    border-radius: 10px;
  }
  .row:hover {
    background: var(--raised-2);
  }
  .info {
    flex: 1;
    display: grid;
    min-width: 0;
  }
  .info b {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .info small {
    color: var(--muted);
  }
  .info .err {
    color: #ff8f86;
  }
  .empty {
    padding: 16px;
    text-align: center;
    color: var(--faint);
  }
  .new {
    margin-right: auto;
  }
</style>
