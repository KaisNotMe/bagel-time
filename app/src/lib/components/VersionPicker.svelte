<script lang="ts">
  import { api, errorMessage, LOADER_NAMES, type Loader, type LoaderVersion, type VersionInfo } from "$lib/api";
  import { ALL_LOADERS } from "$lib/fit";

  type Props = {
    /** Load lists only while shown. */
    active: boolean;
    showSnapshots: boolean;
    version: string;
    loader: Loader;
    loaderVersion: string;
    /** True once a complete, supported choice is made. */
    valid?: boolean;
    /** Only offer these loaders and, per loader, these Minecraft versions. */
    supported?: Map<Loader, Set<string>> | null;
  };
  let {
    active,
    showSnapshots,
    version = $bindable(),
    loader = $bindable(),
    loaderVersion = $bindable(),
    valid = $bindable(false),
    supported = null,
  }: Props = $props();

  let loaders = $derived(supported ? ALL_LOADERS.filter((l) => supported.has(l)) : ALL_LOADERS);

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
    // When limited to what a project supports, it may only support snapshots.
    if (active) loadVersions(snapshots || !!supported);
  });

  /** Versions to offer: releases (plus snapshots if asked), limited to what's supported. */
  let shown = $derived.by(() => {
    if (!supported) return versions;
    const fitting = versions.filter((v) => supported.get(loader)?.has(v.id));
    const picked = fitting.filter((v) => snapshots || v.kind === "release");
    return picked.length > 0 ? picked : fitting;
  });

  /** The supported loader with the newest Minecraft version; earlier loaders win ties. */
  function bestLoader(allowed: Map<Loader, Set<string>>): Loader {
    let best = loaders[0];
    let bestIndex = Infinity;
    for (const l of loaders) {
      const i = versions.findIndex((v) => allowed.get(l)?.has(v.id) && (snapshots || v.kind === "release"));
      if (i !== -1 && i < bestIndex) [best, bestIndex] = [l, i];
    }
    return best;
  }

  // Keep the choice on something the project supports.
  $effect(() => {
    if (!supported || versions.length === 0) return;
    if (!supported.has(loader)) loader = bestLoader(supported);
    if (!shown.some((v) => v.id === version)) version = shown[0]?.id ?? "";
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
  <div class="segmented" role="radiogroup" aria-label="Mod loader" style:--n={loaders.length}>
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
</div>

<div class="row">
  <label class="field grow">
    <span>Minecraft version</span>
    <select class="input" bind:value={version} disabled={loading || shown.length === 0}>
      {#if version && !shown.some((v) => v.id === version)}
        <option value={version}>{version}</option>
      {:else if loading && versions.length === 0}
        <option value="">Loading…</option>
      {/if}
      {#each shown as v (v.id)}
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
    grid-template-columns: repeat(var(--n), 1fr);
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
