<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { api, errorMessage, type ContentKind, type ProjectType, type Source } from "$lib/api";
  import { games } from "$lib/games.svelte";
  import { packs } from "$lib/packs.svelte";
  import Icon from "./Icon.svelte";
  import InstallPicker from "./InstallPicker.svelte";

  type Props = {
    source?: Source;
    projectId: string;
    title: string;
    projectType: ProjectType;
    versionId?: string | null;
    /** Install straight into this instance instead of asking. */
    instanceId?: string | null;
    installed?: boolean;
    small?: boolean;
    /** The author only allows downloads from the website; link there instead. */
    blockedUrl?: string | null;
    oninstalled?: () => void;
  };
  let {
    source = "modrinth",
    projectId,
    title,
    projectType,
    versionId = null,
    instanceId = null,
    installed = false,
    small = false,
    blockedUrl = null,
    oninstalled,
  }: Props = $props();

  let busy = $state(false);
  let error = $state("");
  let picking = $state(false);

  let blocked = $derived(instanceId !== null && !!games.status[instanceId]);

  async function click() {
    error = "";
    if (projectType === "modpack") {
      await packs.install(source, projectId, title, versionId);
      return;
    }
    if (!instanceId) {
      picking = true;
      return;
    }
    busy = true;
    try {
      await api.installContent(instanceId, projectType as ContentKind, source, projectId, versionId);
      oninstalled?.();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="wrap">
  {#if installed && !versionId}
    <span class="installed" class:small><Icon name="check" size={15} /> Installed</span>
  {:else if blockedUrl}
    <button
      class="btn"
      class:sm={small}
      title="The author only allows downloads from the CurseForge website"
      onclick={(e) => {
        e.preventDefault();
        e.stopPropagation();
        openUrl(blockedUrl);
      }}
    >
      <Icon name="external" size={small ? 14 : 16} /> Get on CurseForge
    </button>
  {:else}
    <button
      class="btn primary"
      class:sm={small}
      disabled={busy || blocked || (projectType === "modpack" && packs.busy)}
      title={blocked ? "Close the game first" : undefined}
      onclick={(e) => {
        e.preventDefault();
        e.stopPropagation();
        click();
      }}
    >
      <Icon name="download" size={small ? 14 : 16} />
      {busy || packs.installing === projectId ? "Installing…" : "Install"}
    </button>
  {/if}
  {#if error}<p class="err" title={error}>{error}</p>{/if}
</div>

{#if projectType !== "modpack"}
  <InstallPicker
    open={picking}
    {source}
    {projectId}
    {title}
    kind={projectType as ContentKind}
    {versionId}
    onclose={() => (picking = false)}
  />
{/if}

<style>
  .wrap {
    display: grid;
    justify-items: end;
    gap: 4px;
    max-width: 240px;
  }
  .installed {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 36px;
    padding: 0 12px;
    border-radius: 10px;
    background: var(--ok-soft);
    color: var(--ok);
    font-weight: 600;
  }
  .installed.small {
    height: 30px;
    font-size: 13px;
  }
  .err {
    color: #ff8f86;
    font-size: 12px;
    text-align: right;
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
</style>
