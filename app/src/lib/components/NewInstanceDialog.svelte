<script lang="ts">
  import {
    api,
    errorMessage,
    LOADER_NAMES,
    type Instance,
    type Loader,
    type LoaderVersion,
    type VersionInfo,
  } from "$lib/api";
  import Modal from "./Modal.svelte";

  type Props = {
    open: boolean;
    showSnapshots: boolean;
    onclose: () => void;
    oncreated: (instance: Instance) => void;
  };
  let { open, showSnapshots, onclose, oncreated }: Props = $props();

  const loaders: Loader[] = ["vanilla", "fabric", "quilt"];

  let versions = $state<VersionInfo[]>([]);
  let loading = $state(false);
  let loadError = $state("");
  let snapshots = $state(false);
  let name = $state("");
  let version = $state("");
  let loader = $state<Loader>("vanilla");
  let loaderVersions = $state<LoaderVersion[]>([]);
  let loaderVersion = $state("");
  let loaderLoading = $state(false);
  let loaderError = $state("");
  let creating = $state(false);
  let error = $state("");

  let unsupported = $derived(
    loader !== "vanilla" && !loaderLoading && !loaderError && version !== "" && loaderVersions.length === 0,
  );
  let canCreate = $derived(
    !!version && !creating && (loader === "vanilla" || (!!loaderVersion && !loaderLoading)),
  );
  let placeholder = $derived(
    version ? (loader === "vanilla" ? version : `${LOADER_NAMES[loader]} ${version}`) : "My instance",
  );

  // Start from the user's preference each time the dialog opens.
  $effect(() => {
    if (open) snapshots = showSnapshots;
  });

  $effect(() => {
    if (open) loadVersions(snapshots);
  });

  $effect(() => {
    if (open && loader !== "vanilla" && version) loadLoaderVersions(loader, version);
  });

  async function loadVersions(includeSnapshots: boolean) {
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

  // Ignore responses that arrive after the user picked something else.
  let request = 0;
  async function loadLoaderVersions(forLoader: Loader, forVersion: string) {
    const id = ++request;
    loaderLoading = true;
    loaderError = "";
    loaderVersions = [];
    loaderVersion = "";
    try {
      const list = await api.listLoaderVersions(forLoader, forVersion);
      if (id !== request) return;
      loaderVersions = list;
      loaderVersion = (list.find((v) => v.stable) ?? list[0])?.version ?? "";
    } catch (e) {
      if (id === request) loaderError = `Couldn't load ${LOADER_NAMES[forLoader]} versions: ${errorMessage(e)}`;
    } finally {
      if (id === request) loaderLoading = false;
    }
  }

  async function create(e: SubmitEvent) {
    e.preventDefault();
    creating = true;
    error = "";
    try {
      const instance = await api.createInstance(
        name.trim() || placeholder,
        version,
        loader,
        loader === "vanilla" ? null : loaderVersion,
      );
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
      <input class="input" bind:value={name} {placeholder} maxlength="40" />
    </label>

    <div class="field">
      <span>Mod loader</span>
      <div class="segmented" role="radiogroup" aria-label="Mod loader">
        {#each loaders as l (l)}
          <button
            type="button"
            role="radio"
            aria-checked={loader === l}
            class:selected={loader === l}
            onclick={() => (loader = l)}>{LOADER_NAMES[l]}</button
          >
        {/each}
      </div>
      <small>Forge and NeoForge are on the way.</small>
    </div>

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

    {#if loader !== "vanilla"}
      <label class="field">
        <span>{LOADER_NAMES[loader]} version</span>
        <select class="input" bind:value={loaderVersion} disabled={loaderLoading || loaderVersions.length === 0}>
          {#if loaderLoading}
            <option value="">Loading…</option>
          {:else if loaderVersions.length === 0}
            <option value="">Not available</option>
          {/if}
          {#each loaderVersions as v (v.version)}
            <option value={v.version}>{v.version}{v.stable ? "" : "  (beta)"}</option>
          {/each}
        </select>
        {#if unsupported}
          <small class="warn">{LOADER_NAMES[loader]} doesn't support Minecraft {version} yet.</small>
        {/if}
      </label>
    {/if}

    {#if loadError}<p class="alert">{loadError}</p>{/if}
    {#if loaderError}<p class="alert">{loaderError}</p>{/if}
    {#if error}<p class="alert">{error}</p>{/if}
  </form>

  {#snippet footer()}
    <button class="btn ghost" type="button" onclick={onclose}>Cancel</button>
    <button class="btn primary" type="submit" form="new-instance" disabled={!canCreate}>
      {creating ? "Creating…" : "Create"}
    </button>
  {/snippet}
</Modal>

<style>
  .form {
    display: grid;
    gap: 14px;
  }
  .segmented {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 4px;
    padding: 3px;
    border: 1px solid var(--line-strong);
    border-radius: 9px;
    background: var(--bg);
  }
  .segmented button {
    height: 30px;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--muted);
    font-weight: 600;
    cursor: pointer;
  }
  .segmented button:hover {
    color: var(--text);
  }
  .segmented button.selected {
    background: var(--card-hover);
    color: var(--text);
    box-shadow: inset 0 0 0 1px var(--line-strong);
  }
  .warn {
    color: var(--warn) !important;
  }
</style>
