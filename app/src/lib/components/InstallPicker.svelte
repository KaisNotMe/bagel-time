<script lang="ts">
  import { api, errorMessage, LOADER_NAMES, PROJECT_TYPE_NAMES, type ContentKind, type Fit, type Source } from "$lib/api";
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
  /** Per instance id; null while checking. */
  let fits = $state<Record<string, Fit> | null>(null);

  $effect(() => {
    if (!open) return;
    rows = {};
    fits = null;
    api
      .checkContentFit(kind, source, projectId, versionId)
      .then((f) => (fits = f))
      // Couldn't check: let every instance try, installing shows the real error.
      .catch(() => (fits = {}));
  });

  function fitOf(id: string): Fit | null {
    if (!fits) return null;
    return fits[id] ?? { fits: true, version: null, reason: null };
  }

  // Ones that fit first; the rest keep their order.
  let sorted = $derived(
    fits ? [...instances.list].sort((a, b) => Number(fitOf(b.id)!.fits) - Number(fitOf(a.id)!.fits)) : instances.list,
  );
  let noneFit = $derived(fits !== null && instances.list.length > 0 && instances.list.every((i) => !fitOf(i.id)!.fits));

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
  <p class="muted">
    Choose an instance to add this {PROJECT_TYPE_NAMES[kind].one.toLowerCase()} to. Instances it won't work in are greyed
    out.
  </p>
  {#if noneFit}
    <p class="note">
      <Icon name="info" size={15} /> None of your instances can use {versionId ? "this version" : "it"}. Make a new
      instance with a Minecraft version and loader it supports.
    </p>
  {/if}
  <div class="list">
    {#each sorted as inst (inst.id)}
      {@const row = rows[inst.id] ?? {}}
      {@const fit = fitOf(inst.id)}
      <div class="row" class:off={fit && !fit.fits && !row.done}>
        <InstanceIcon instance={inst} size={40} />
        <div class="info">
          <b>{inst.name}</b>
          <small>
            {LOADER_NAMES[inst.loader]} {inst.gameVersion}
            {#if fit?.fits && fit.version}<span class="ver"> · gets {fit.version}</span>{/if}
          </small>
          {#if row.error}<small class="err">{row.error}</small>{/if}
        </div>
        {#if row.done}
          <span class="tag ok"><Icon name="check" size={13} /> Installed</span>
        {:else if !fit}
          <span class="checking">Checking…</span>
        {:else if !fit.fits}
          <span class="why" title={fit.reason ?? undefined}>{fit.reason ?? "Won't work"}</span>
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
      <p class="empty">You don't have any instances yet.</p>
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
  .row.off {
    opacity: 0.45;
    filter: grayscale(1);
  }
  .row.off:hover {
    background: none;
  }
  .info {
    flex: 1;
    display: grid;
    min-width: 0;
  }
  .info b,
  .info small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .info small {
    color: var(--muted);
  }
  .info .ver {
    color: var(--faint);
  }
  .info .err {
    color: #ff8f86;
    white-space: normal;
  }
  .why,
  .checking {
    flex-shrink: 0;
    max-width: 190px;
    font-size: 12px;
    color: var(--faint);
    text-align: right;
  }
  .note {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 10px;
    padding: 10px 12px;
    border-radius: 10px;
    background: rgb(233 196 106 / 0.12);
    color: var(--warn);
    font-size: 13px;
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
