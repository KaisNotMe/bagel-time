<script lang="ts">
  import { api, errorMessage, LOADER_NAMES, PROJECT_TYPE_NAMES, type Instance, type Loader } from "$lib/api";
  import { supportedGames } from "$lib/fit";
  import { packs } from "$lib/packs.svelte";
  import type { CreatingFor } from "$lib/ui.svelte";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";
  import VersionPicker from "./VersionPicker.svelte";

  type Props = {
    open: boolean;
    showSnapshots: boolean;
    /** Set up for installing this project: only fitting loaders and versions are offered. */
    forProject?: CreatingFor | null;
    onclose: () => void;
    oncreated: (instance: Instance) => void;
  };
  let { open, showSnapshots, forProject = null, onclose, oncreated }: Props = $props();

  let name = $state("");
  let version = $state("");
  let loader = $state<Loader>("vanilla");
  let loaderVersion = $state("");
  let valid = $state(false);
  let creating = $state(false);
  let error = $state("");
  /** What the project supports; null while loading or when not for a project. */
  let supported = $state<Map<Loader, Set<string>> | null>(null);
  let checking = $state(false);
  /** Created, but installing the project into it failed. */
  let created = $state<Instance | null>(null);

  let placeholder = $derived(
    version ? (loader === "vanilla" ? version : `${LOADER_NAMES[loader]} ${version}`) : "My instance",
  );
  let nothingFits = $derived(supported !== null && supported.size === 0);

  $effect(() => {
    if (!open) return;
    error = "";
    created = null;
    supported = null;
    const p = forProject;
    if (!p) return;
    checking = true;
    api
      .getProjectVersions(p.source, p.projectId)
      .then((list) => {
        const versions = p.versionId ? list.filter((v) => v.id === p.versionId) : list;
        supported = supportedGames(versions, p.kind);
      })
      // Couldn't check: offer everything, installing shows the real error.
      .catch(() => {})
      .finally(() => (checking = false));
  });

  async function create(e: SubmitEvent) {
    e.preventDefault();
    if (created) {
      oncreated(created);
      return;
    }
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
      if (forProject) {
        try {
          await api.installContent(
            instance.id,
            forProject.kind,
            forProject.source,
            forProject.projectId,
            forProject.versionId,
          );
        } catch (err) {
          created = instance;
          error = `Created ${instance.name}, but couldn't install ${forProject.title}: ${errorMessage(err)}`;
          return;
        }
      }
      oncreated(instance);
    } catch (err) {
      error = errorMessage(err);
    } finally {
      creating = false;
    }
  }
</script>

<Modal {open} title={forProject ? `New instance for ${forProject.title}` : "New instance"} {onclose}>
  <form id="new-instance" class="form" onsubmit={create}>
    {#if forProject}
      <p class="guide">
        <Icon name="info" size={15} />
        {#if checking}
          Checking which versions {forProject.title} works with…
        {:else if nothingFits}
          {forProject.title} has no {forProject.versionId ? "Minecraft version" : "versions"} Bagel Time can install.
        {:else}
          Only loaders and Minecraft versions this {PROJECT_TYPE_NAMES[forProject.kind].one.toLowerCase()} works
          with are shown. It's installed as soon as the instance is created.
        {/if}
      </p>
    {/if}
    <label class="field">
      <span>Name</span>
      <input class="input" bind:value={name} {placeholder} maxlength="60" />
    </label>
    {#if !nothingFits}
      <VersionPicker
        active={open && !checking}
        {showSnapshots}
        {supported}
        bind:version
        bind:loader
        bind:loaderVersion
        bind:valid
      />
    {/if}
    {#if error}<p class="alert">{error}</p>{/if}
  </form>

  {#snippet footer()}
    {#if !forProject}
      <button
        class="btn ghost import"
        type="button"
        disabled={packs.busy}
        onclick={() => {
          onclose();
          packs.pickFile();
        }}>Import modpack file…</button
      >
    {/if}
    <button class="btn ghost" type="button" onclick={onclose}>Cancel</button>
    <button
      class="btn primary"
      type="submit"
      form="new-instance"
      disabled={!created && (!valid || creating || checking || nothingFits)}
    >
      {#if created}
        Open instance
      {:else if creating}
        {forProject ? "Creating and installing…" : "Creating…"}
      {:else}
        {forProject ? "Create and install" : "Create"}
      {/if}
    </button>
  {/snippet}
</Modal>

<style>
  .form {
    display: grid;
    gap: 14px;
  }
  .guide {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
    border-radius: 10px;
    background: var(--raised-2);
    color: var(--muted);
    font-size: 13px;
  }
  .import {
    margin-right: auto;
  }
</style>
