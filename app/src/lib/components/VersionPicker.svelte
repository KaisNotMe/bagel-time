<script lang="ts">
  import { api, errorMessage, LOADER_NAMES, type Loader, type LoaderVersion, type VersionInfo } from "$lib/api";

  type Props = {
    /** Load lists only while shown. */
    active: boolean;
    showSnapshots: boolean;
    version: string;
    loader: Loader;
    loaderVersion: string;
    /** True once a complete, supported choice is made. */
    valid?: boolean;
  };
  let {
    active,
    showSnapshots,
    version = $bindable(),
    loader = $bindable(),
    loaderVersion = $bindable(),
    valid = $bindable(false),
  }: Props = $props();

  const loaders: Loader[] = ["vanilla", "fabric", "quilt"];

  let versions = $state<VersionInfo[]>([]);
  let loading = $state(false);
  let loadError = $state("");
  let snapshots = $state(false);
  let loaderVersions = $state<LoaderVersion[]>([]);
  let loaderLoading = $state(false);
  let loaderError = $state("");

  let unsupported = $derived(
    loader !== "vanilla" && !loaderLoading && !loaderError && version !== "" && loaderVersions.length === 0,
  );

  $effect(() => {
    valid = !!version && (loader === "vanilla" || (!!loaderVersion && !loaderLoading));
  });

  // Start from the user's preference each time it's shown.
  $effect(() => {
    if (active) snapshots = showSnapshots;
  });

  $effect(() => {
    if (active) loadVersions(snapshots);
  });

  $effect(() => {
    if (active && loader !== "vanilla" && version) loadLoaderVersions(loader, version);
  });

  async function loadVersions(includeSnapshots: boolean) {
    loading = true;
    loadError = "";
    try {
      versions = await api.listVersions(includeSnapshots);
      if (!version) {
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
    const keep = loaderVersion;
    loaderLoading = true;
    loaderError = "";
    loaderVersions = [];
    try {
      const list = await api.listLoaderVersions(forLoader, forVersion);
      if (id !== request) return;
      loaderVersions = list;
      loaderVersion = list.some((v) => v.version === keep)
        ? keep
        : ((list.find((v) => v.stable) ?? list[0])?.version ?? "");
    } catch (e) {
      if (id === request) loaderError = `Couldn't load ${LOADER_NAMES[forLoader]} versions: ${errorMessage(e)}`;
    } finally {
      if (id === request) loaderLoading = false;
    }
  }
</script>

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

<div class="row">
  <label class="field grow">
    <span>Minecraft version</span>
    <select class="input" bind:value={version} disabled={loading || versions.length === 0}>
      {#if version && !versions.some((v) => v.id === version)}
        <option value={version}>{version}</option>
      {:else if loading && versions.length === 0}
        <option value="">Loading…</option>
      {/if}
      {#each versions as v (v.id)}
        <option value={v.id}>{v.id}{v.kind === "snapshot" ? "  (snapshot)" : ""}</option>
      {/each}
    </select>
  </label>

  {#if loader !== "vanilla"}
    <label class="field grow">
      <span>{LOADER_NAMES[loader]} version</span>
      <select class="input" bind:value={loaderVersion} disabled={loaderLoading || loaderVersions.length === 0}>
        {#if loaderLoading}
          <option value={loaderVersion}>Loading…</option>
        {:else if loaderVersions.length === 0}
          <option value="">Not available</option>
        {/if}
        {#each loaderVersions as v (v.version)}
          <option value={v.version}>{v.version}{v.stable ? "" : "  (beta)"}</option>
        {/each}
      </select>
    </label>
  {/if}
</div>
<label class="check"><input type="checkbox" bind:checked={snapshots} /> Show snapshots</label>

{#if unsupported}
  <small class="warn">{LOADER_NAMES[loader]} doesn't support Minecraft {version} yet.</small>
{/if}
{#if loadError}<p class="alert">{loadError}</p>{/if}
{#if loaderError}<p class="alert">{loaderError}</p>{/if}

<style>
  .segmented {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 4px;
    padding: 4px;
    border-radius: 12px;
    background: var(--raised-2);
  }
  .segmented button {
    height: 30px;
    border: 0;
    border-radius: 8px;
    background: none;
    color: var(--muted);
    font-weight: 600;
    cursor: pointer;
  }
  .segmented button:hover {
    color: var(--text);
  }
  .segmented button.selected {
    background: var(--raised-3);
    color: var(--text);
  }
  .row {
    display: flex;
    gap: 10px;
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .warn {
    color: var(--warn);
  }
</style>
