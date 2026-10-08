<script lang="ts">
  import { api, errorMessage, type Instance, type VersionInfo } from "$lib/api";
  import Modal from "./Modal.svelte";

  type Props = {
    open: boolean;
    showSnapshots: boolean;
    onclose: () => void;
    oncreated: (instance: Instance) => void;
  };
  let { open, showSnapshots, onclose, oncreated }: Props = $props();

  let versions = $state<VersionInfo[]>([]);
  let loading = $state(false);
  let loadError = $state("");
  let snapshots = $state(false);
  let name = $state("");
  let version = $state("");
  let creating = $state(false);
  let error = $state("");

  // Start from the user's preference each time the dialog opens.
  $effect(() => {
    if (open) snapshots = showSnapshots;
  });

  $effect(() => {
    if (open) load(snapshots);
  });

  async function load(includeSnapshots: boolean) {
    loading = true;
    loadError = "";
    try {
      versions = await api.listVersions(includeSnapshots);
      if (!versions.some((v) => v.id === version)) {
        version = versions.find((v) => v.kind === "release")?.id ?? versions[0]?.id ?? "";
      }
    } catch (e) {
      loadError = `Couldn't load the version list: ${errorMessage(e)}`;
    } finally {
      loading = false;
    }
  }

  async function create(e: SubmitEvent) {
    e.preventDefault();
    creating = true;
    error = "";
    try {
      const instance = await api.createInstance(name.trim() || version, version);
      name = "";
      oncreated(instance);
    } catch (err) {
      error = errorMessage(err);
    } finally {
      creating = false;
    }
  }
</script>

<Modal {open} title="New instance" {onclose}>
  <form id="new-instance" class="form" onsubmit={create}>
    <label class="field">
      <span>Name</span>
      <input class="input" bind:value={name} placeholder={version || "My instance"} maxlength="40" />
    </label>
    <label class="field">
      <span>Minecraft version</span>
      <select class="input" bind:value={version} disabled={loading || versions.length === 0}>
        {#if loading && versions.length === 0}
          <option value="">Loading…</option>
        {/if}
        {#each versions as v (v.id)}
          <option value={v.id}>{v.id}{v.kind === "snapshot" ? "  (snapshot)" : ""}</option>
        {/each}
      </select>
    </label>
    <label class="check"><input type="checkbox" bind:checked={snapshots} /> Show snapshots</label>
    <label class="field">
      <span>Mod loader</span>
      <select class="input" disabled><option>Vanilla</option></select>
      <small>Fabric, Quilt, Forge and NeoForge are on the way.</small>
    </label>
    {#if loadError}<p class="alert">{loadError}</p>{/if}
    {#if error}<p class="alert">{error}</p>{/if}
  </form>

  {#snippet footer()}
    <button class="btn ghost" type="button" onclick={onclose}>Cancel</button>
    <button class="btn primary" type="submit" form="new-instance" disabled={!version || creating}>
      {creating ? "Creating…" : "Create"}
    </button>
  {/snippet}
</Modal>

<style>
  .form {
    display: grid;
    gap: 14px;
  }
</style>
