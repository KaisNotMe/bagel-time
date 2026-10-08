<script lang="ts">
  import { goto } from "$app/navigation";
  import { page } from "$app/state";
  import { api, errorMessage, LOADER_NAMES, type Instance } from "$lib/api";
  import Icon, { type IconName } from "$lib/components/Icon.svelte";
  import InstanceIcon from "$lib/components/InstanceIcon.svelte";
  import InstanceSettings from "$lib/components/InstanceSettings.svelte";
  import ContentTab from "$lib/components/instance/ContentTab.svelte";
  import LogsTab from "$lib/components/instance/LogsTab.svelte";
  import ScreenshotsTab from "$lib/components/instance/ScreenshotsTab.svelte";
  import WorldsTab from "$lib/components/instance/WorldsTab.svelte";
  import { relativeTime } from "$lib/format";
  import { games } from "$lib/games.svelte";
  import { instances } from "$lib/instances.svelte";
  import { ui } from "$lib/ui.svelte";

  type Tab = "content" | "worlds" | "logs" | "screenshots";
  const tabs: { id: Tab; label: string; icon: IconName }[] = [
    { id: "content", label: "Content", icon: "puzzle" },
    { id: "worlds", label: "Worlds", icon: "globe" },
    { id: "logs", label: "Logs", icon: "terminal" },
    { id: "screenshots", label: "Screenshots", icon: "camera" },
  ];

  let id = $derived(page.url.searchParams.get("id") ?? "");
  let tab = $derived((page.url.searchParams.get("tab") as Tab | null) ?? "content");

  let fetched = $state<Instance | null>(null);
  let loadError = $state("");
  let settingsOpen = $state(false);

  // Prefer the shared list so edits elsewhere show up immediately.
  let instance = $derived(instances.get(id) ?? (fetched?.id === id ? fetched : null));
  let status = $derived(instance ? games.status[instance.id] : undefined);
  let running = $derived(!!status);
  let percent = $derived(
    status?.phase === "preparing" && status.total > 0 ? Math.round((status.done / status.total) * 100) : null,
  );

  $effect(() => {
    if (!id) return;
    loadError = "";
    api
      .getInstance(id)
      .then((i) => (fetched = i))
      .catch((e) => (loadError = errorMessage(e)));
  });

  $effect(() => {
    ui.setCrumbs({ label: "Library", href: "/library" }, { label: instance?.name ?? "Instance" });
  });

  function setTab(t: Tab) {
    const url = new URL(page.url);
    url.searchParams.set("tab", t);
    goto(url, { replaceState: true, keepFocus: true, noScroll: true });
  }
</script>

<div class="page">
  <div class="page-inner">
    {#if loadError}
      <p class="alert">Couldn't open this instance: {loadError}</p>
    {:else if instance}
      <header class="hero card">
        <InstanceIcon {instance} size={96} radius={20} />
        <div class="info">
          <h1>{instance.name}</h1>
          <p class="meta">
            <span class="chip"><Icon name={instance.loader === "vanilla" ? "box" : "puzzle"} size={14} />{LOADER_NAMES[instance.loader]} {instance.gameVersion}</span>
            {#if instance.loaderVersion}<span class="chip">Loader {instance.loaderVersion}</span>{/if}
            <span class="chip"><Icon name="clock" size={14} />{relativeTime(instance.lastPlayed)}</span>
          </p>
          {#if games.errors[instance.id]}
            <div class="alert row err">
              <span>{games.errors[instance.id]}</span>
              <button class="btn ghost sm" onclick={() => games.dismissError(instance!.id)}>Dismiss</button>
            </div>
          {/if}
        </div>
        <div class="actions">
          {#if status?.phase === "running"}
            <button class="btn lg danger" onclick={() => games.stop(instance!.id)}>
              <Icon name="stop" size={16} fill /> Stop
            </button>
          {:else}
            <button class="btn lg primary play" disabled={running} onclick={() => games.launch(instance!.id)}>
              <Icon name="play" size={17} fill />
              {#if status}{status.stage || "Starting"}{percent !== null ? ` ${percent}%` : "…"}{:else}Play{/if}
            </button>
          {/if}
          <button class="btn lg square" title="Settings" aria-label="Instance settings" onclick={() => (settingsOpen = true)}>
            <Icon name="settings" size={19} />
          </button>
          <button class="btn lg square" title="Open folder" aria-label="Open instance folder" onclick={() => api.openInstanceFolder(instance!.id)}>
            <Icon name="folder" size={19} />
          </button>
        </div>
      </header>

      <nav class="pills" aria-label="Instance sections">
        {#each tabs as t (t.id)}
          <button class="pill" class:active={tab === t.id} onclick={() => setTab(t.id)}>
            <Icon name={t.icon} size={15} />
            {t.label}
          </button>
        {/each}
      </nav>

      <div class="tab-body">
        {#if tab === "content"}
          <ContentTab {instance} {running} />
        {:else if tab === "worlds"}
          <WorldsTab {instance} />
        {:else if tab === "logs"}
          <LogsTab {instance} />
        {:else}
          <ScreenshotsTab {instance} />
        {/if}
      </div>

      <InstanceSettings
        open={settingsOpen}
        {instance}
        onclose={() => (settingsOpen = false)}
        ondeleted={() => {
          settingsOpen = false;
          goto("/library");
        }}
        onduplicated={(copy) => {
          settingsOpen = false;
          goto(`/instance?id=${encodeURIComponent(copy.id)}`);
        }}
      />
    {/if}
  </div>
</div>

<style>
  .hero {
    display: flex;
    align-items: center;
    gap: 20px;
    padding: 20px;
  }
  .info {
    flex: 1;
    min-width: 0;
    display: grid;
    gap: 8px;
  }
  h1 {
    font-size: 26px;
    letter-spacing: -0.4px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    padding: 0 10px;
    border-radius: 999px;
    background: var(--raised-2);
    color: var(--muted);
    font-size: 12.5px;
    font-weight: 600;
  }
  .err {
    font-size: 13px;
  }
  .actions {
    display: flex;
    gap: 8px;
    align-self: center;
  }
  .play {
    min-width: 130px;
  }
  .tab-body {
    min-height: 0;
  }
</style>
