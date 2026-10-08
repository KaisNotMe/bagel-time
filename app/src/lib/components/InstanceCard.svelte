<script lang="ts">
  import { LOADER_NAMES, type Instance } from "$lib/api";
  import { relativeTime } from "$lib/format";
  import { games } from "$lib/games.svelte";
  import Icon from "./Icon.svelte";
  import InstanceIcon from "./InstanceIcon.svelte";

  type Props = {
    instance: Instance;
    onmenu: (action: "folder" | "duplicate" | "settings" | "delete") => void;
  };
  let { instance, onmenu }: Props = $props();

  let status = $derived(games.status[instance.id]);
  let error = $derived(games.errors[instance.id]);
  let href = $derived(`/instance?id=${encodeURIComponent(instance.id)}`);
  let percent = $derived(
    status?.phase === "preparing" && status.total > 0
      ? Math.min(100, Math.round((status.done / status.total) * 100))
      : null,
  );

  let menuOpen = $state(false);
  let menuWrap = $state<HTMLElement>();

  function pick(action: Parameters<Props["onmenu"]>[0]) {
    menuOpen = false;
    onmenu(action);
  }
</script>

<svelte:window
  onclick={(e) => {
    if (menuOpen && menuWrap && !menuWrap.contains(e.target as Node)) menuOpen = false;
  }}
/>

<article class="card" class:active={!!status}>
  <div class="icon-wrap">
    <a {href} tabindex="-1" aria-hidden="true"><InstanceIcon {instance} size={72} radius={16} /></a>
    {#if status?.phase === "running"}
      <button class="fab stop" aria-label="Stop {instance.name}" onclick={() => games.stop(instance.id)}>
        <Icon name="stop" size={14} fill />
      </button>
    {:else if !status}
      <button class="fab play" aria-label="Play {instance.name}" onclick={() => games.launch(instance.id)}>
        <Icon name="play" size={16} fill />
      </button>
    {/if}
  </div>

  <a class="info" {href}>
    <h3 title={instance.name}>{instance.name}</h3>
    <p class="meta">
      <Icon name={instance.loader === "vanilla" ? "box" : "puzzle"} size={13} />
      {LOADER_NAMES[instance.loader]}
      {instance.gameVersion}
    </p>
    <p class="meta faint">
      {#if status?.phase === "running"}
        <span class="live"></span> Playing now
      {:else if status}
        {status.stage || "Starting"}{percent !== null ? ` · ${percent}%` : ""}
      {:else}
        {relativeTime(instance.lastPlayed)}
      {/if}
    </p>
  </a>

  <div class="menu-wrap" bind:this={menuWrap}>
    <button
      class="btn ghost sm square"
      aria-label="More actions for {instance.name}"
      aria-expanded={menuOpen}
      onclick={() => (menuOpen = !menuOpen)}
    >
      <Icon name="more" size={20} stroke={3} />
    </button>
    {#if menuOpen}
      <div class="menu" role="menu">
        <button role="menuitem" onclick={() => pick("settings")}><Icon name="settings" size={15} /> Settings</button>
        <button role="menuitem" onclick={() => pick("folder")}><Icon name="folder" size={15} /> Open folder</button>
        <button role="menuitem" disabled={!!status} onclick={() => pick("duplicate")}>
          <Icon name="copyplus" size={15} /> Duplicate
        </button>
        <button role="menuitem" class="danger-text" disabled={!!status} onclick={() => pick("delete")}>
          <Icon name="trash" size={15} /> Delete…
        </button>
      </div>
    {/if}
  </div>

  {#if status?.phase === "preparing"}
    <div class="bar" aria-hidden="true">
      <div class="fill" class:indeterminate={percent === null} style:width="{percent ?? 35}%"></div>
    </div>
  {/if}

  {#if error}
    <div class="error">
      <p>{error}</p>
      <button class="link" onclick={() => games.dismissError(instance.id)}>Dismiss</button>
    </div>
  {/if}
</article>

<style>
  .card {
    position: relative;
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 14px;
    padding: 12px;
    border-radius: var(--radius);
    background: var(--raised);
    transition: background 0.12s, box-shadow 0.12s;
  }
  .card:hover {
    background: var(--raised-2);
  }
  .card.active {
    box-shadow: inset 0 0 0 1.5px color-mix(in srgb, var(--accent) 60%, transparent);
  }
  .icon-wrap {
    position: relative;
  }
  .icon-wrap a {
    display: block;
    text-decoration: none;
  }
  .fab {
    position: absolute;
    right: -6px;
    bottom: -6px;
    display: grid;
    place-items: center;
    width: 34px;
    height: 34px;
    border: 3px solid var(--raised-2);
    border-radius: 50%;
    cursor: pointer;
    opacity: 0;
    transform: scale(0.85);
    transition: opacity 0.12s, transform 0.12s;
  }
  .fab.play {
    background: var(--accent);
    color: var(--accent-ink);
    padding-left: 2px;
  }
  .fab.stop {
    background: var(--danger);
    color: #fff;
    opacity: 1;
    transform: none;
    border-color: var(--raised);
  }
  .card:hover .fab,
  .fab:focus-visible {
    opacity: 1;
    transform: none;
  }
  .info {
    display: grid;
    gap: 3px;
    min-width: 0;
    color: inherit;
    text-decoration: none;
  }
  h3 {
    font-size: 15px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .info:hover h3 {
    color: var(--accent);
  }
  .meta {
    display: flex;
    align-items: center;
    gap: 5px;
    color: var(--muted);
    font-size: 13px;
    white-space: nowrap;
    overflow: hidden;
  }
  .live {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--ok);
    box-shadow: 0 0 0 3px var(--ok-soft);
  }
  .menu-wrap {
    position: relative;
    align-self: start;
    color: var(--muted);
  }
  .menu {
    position: absolute;
    right: 0;
    top: calc(100% + 4px);
    z-index: 10;
    min-width: 170px;
    padding: 5px;
    border-radius: 12px;
    background: var(--raised-3);
    box-shadow: var(--shadow);
    display: grid;
  }
  .menu button {
    display: flex;
    align-items: center;
    gap: 9px;
    text-align: left;
    padding: 8px 10px;
    border: 0;
    border-radius: 8px;
    background: none;
    color: var(--text);
    cursor: pointer;
  }
  .menu button:hover:not(:disabled) {
    background: var(--raised-2);
  }
  .menu button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .danger-text {
    color: var(--danger) !important;
  }
  .bar {
    grid-column: 1 / -1;
    height: 5px;
    border-radius: 3px;
    background: var(--raised-3);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: var(--accent);
    transition: width 0.2s;
  }
  .fill.indeterminate {
    animation: slide 1.1s ease-in-out infinite;
  }
  @keyframes slide {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(300%);
    }
  }
  .error {
    grid-column: 1 / -1;
    display: grid;
    gap: 4px;
    padding: 8px 10px;
    border-radius: 8px;
    background: var(--danger-soft);
    color: #ffc9c3;
    font-size: 12.5px;
    word-break: break-word;
  }
  .link {
    justify-self: start;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    text-decoration: underline;
    cursor: pointer;
    font-size: 12px;
  }
</style>
