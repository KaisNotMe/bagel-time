<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { api } from "$lib/api";
  import { packs } from "$lib/packs.svelte";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";

  let manual = $derived(packs.manual);
  let folders = $derived([...new Set(manual?.files.map((f) => f.folder) ?? [])]);
</script>

<Modal open={manual !== null} title="A few files need downloading by hand" onclose={() => (packs.manual = null)} width={600}>
  {#if manual}
    <p class="muted">
      These can only be downloaded from the CurseForge website. Download each one and put it in the
      {folders.length === 1 ? `${folders[0]} folder` : "folder shown under it"}.
    </p>
    <div class="list">
      {#each manual.files as f (f.fileName)}
        <div class="row">
          <div class="info">
            <b>{f.title}</b>
            <small>{f.fileName} · goes in {f.folder}</small>
          </div>
          <button class="btn sm" onclick={() => openUrl(f.url)}>
            <Icon name="external" size={14} /> Download
          </button>
        </div>
      {/each}
    </div>
  {/if}
  {#snippet footer()}
    {#each folders as folder (folder)}
      <button class="btn ghost" onclick={() => manual && api.openInstanceFolder(manual.instance.id, folder)}>
        <Icon name="folder" size={15} /> Open {folder} folder
      </button>
    {/each}
    <button class="btn primary" onclick={() => (packs.manual = null)}>Done</button>
  {/snippet}
</Modal>

<style>
  .list {
    display: grid;
    gap: 2px;
    max-height: 50vh;
    overflow: auto;
    margin-top: 12px;
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
  .info small {
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
