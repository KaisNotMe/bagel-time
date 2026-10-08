<script lang="ts">
  import type { Instance } from "$lib/api";
  import { iconFor, relativeTime } from "$lib/format";
  import { games } from "$lib/games.svelte";

  type Props = {
    instance: Instance;
    ondelete: () => void;
    onopenfolder: () => void;
  };
  let { instance, ondelete, onopenfolder }: Props = $props();

  let status = $derived(games.status[instance.id]);
  let error = $derived(games.errors[instance.id]);
  let icon = $derived(iconFor(instance.name));
  let percent = $derived(
    status?.phase === "preparing" && status.total > 0
      ? Math.min(100, Math.round((status.done / status.total) * 100))
      : null,
  );

  let menuOpen = $state(false);
  let menuWrap = $state<HTMLElement>();
</script>

<svelte:window
  onclick={(e) => {
    if (menuOpen && menuWrap && !menuWrap.contains(e.target as Node)) menuOpen = false;
  }}
/>

<article class="card" class:active={!!status}>
  <div class="top">
    <div class="icon" style:background={icon.background}>{icon.initials}</div>
    <div class="info">
      <h3 title={instance.name}>{instance.name}</h3>
      <p class="meta">Vanilla {instance.gameVersion}</p>
      <p class="meta faint">
        {#if status?.phase === "running"}
          <span class="live"></span> Playing now
        {:else}
          {relativeTime(instance.lastPlayed)}
        {/if}
      </p>
    </div>
  </div>

  {#if status?.phase === "preparing"}
    <div class="progress" aria-live="polite">
      <div class="progress-label">
        <span>{status.stage}</span>
        {#if percent !== null}<span>{percent}%</span>{/if}
      </div>
      <div class="bar">
        <div class="fill" class:indeterminate={percent === null} style:width="{percent ?? 35}%"></div>
      </div>
    </div>
  {/if}

  {#if error}
    <div class="error">
      <p>{error}</p>
      <button class="link" onclick={() => games.dismissError(instance.id)}>Dismiss</button>
    </div>
  {/if}

  <div class="actions">
    {#if status?.phase === "running"}
      <button class="btn danger grow" onclick={() => games.stop(instance.id)}>Stop</button>
    {:else}
      <button class="btn primary grow" disabled={!!status} onclick={() => games.launch(instance.id)}>
        {status ? "Starting…" : "Play"}
      </button>
    {/if}
    <button
      class="btn square"
      title="Game log"
      aria-label="Game log"
      onclick={() => (games.logOpenFor = instance.id)}
    >
      <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6">
        <path d="M3 4h10M3 8h10M3 12h6" stroke-linecap="round" />
      </svg>
    </button>
    <div class="menu-wrap" bind:this={menuWrap}>
      <button class="btn square" aria-label="More actions" aria-expanded={menuOpen} onclick={() => (menuOpen = !menuOpen)}>
        <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
          <circle cx="3.5" cy="8" r="1.4" /><circle cx="8" cy="8" r="1.4" /><circle cx="12.5" cy="8" r="1.4" />
        </svg>
      </button>
      {#if menuOpen}
        <div class="menu" role="menu">
          <button role="menuitem" onclick={() => ((menuOpen = false), onopenfolder())}>Open folder</button>
          <button
            role="menuitem"
            class="danger-text"
            disabled={!!status}
            onclick={() => ((menuOpen = false), ondelete())}>Delete…</button
          >
        </div>
      {/if}
    </div>
  </div>
</article>

<style>
  .card {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 14px;
    border: 1px solid var(--line);
    border-radius: var(--radius);
    background: var(--card);
    transition: border-color 0.15s, background 0.15s;
  }
  .card:hover {
    background: var(--card-hover);
  }
  .card.active {
    border-color: color-mix(in srgb, var(--accent) 55%, var(--line));
  }
  .top {
    display: flex;
    gap: 12px;
    min-width: 0;
  }
  .icon {
    flex: none;
    width: 56px;
    height: 56px;
    border-radius: 12px;
    display: grid;
    place-items: center;
    font-weight: 800;
    font-size: 20px;
    color: rgb(255 255 255 / 0.95);
    text-shadow: 0 1px 2px rgb(0 0 0 / 0.35);
  }
  .info {
    min-width: 0;
    display: grid;
    align-content: center;
    gap: 2px;
  }
  h3 {
    font-size: 15px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta {
    color: var(--muted);
    font-size: 13px;
  }
  .faint {
    color: var(--faint);
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .live {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--ok);
    box-shadow: 0 0 0 3px rgb(123 196 127 / 0.2);
  }
  .progress {
    display: grid;
    gap: 6px;
  }
  .progress-label {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    font-size: 12px;
    color: var(--muted);
  }
  .progress-label span:first-child {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .bar {
    height: 6px;
    border-radius: 3px;
    background: var(--line);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: var(--accent);
    border-radius: 3px;
    transition: width 0.2s;
  }
  .fill.indeterminate {
    animation: slide 1.1s ease-in-out infinite;
  }
  @keyframes slide {
    from { transform: translateX(-100%); }
    to { transform: translateX(300%); }
  }
  .error {
    display: grid;
    gap: 4px;
    padding: 8px 10px;
    border-radius: 8px;
    background: var(--danger-bg);
    color: #ffc9c3;
    font-size: 12.5px;
    word-break: break-word;
  }
  .link {
    justify-self: start;
    padding: 0;
    border: 0;
    background: none;
    color: #ffc9c3;
    text-decoration: underline;
    cursor: pointer;
    font-size: 12px;
  }
  .actions {
    display: flex;
    gap: 6px;
    margin-top: auto;
  }
  .grow {
    flex: 1;
  }
  .menu-wrap {
    position: relative;
  }
  .menu {
    position: absolute;
    right: 0;
    bottom: calc(100% + 6px);
    z-index: 10;
    min-width: 150px;
    padding: 4px;
    border: 1px solid var(--line-strong);
    border-radius: 10px;
    background: var(--panel);
    box-shadow: 0 12px 30px rgb(0 0 0 / 0.45);
    display: grid;
  }
  .menu button {
    text-align: left;
    padding: 8px 10px;
    border: 0;
    border-radius: 6px;
    background: none;
    cursor: pointer;
  }
  .menu button:hover:not(:disabled) {
    background: var(--card-hover);
  }
  .menu button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .danger-text {
    color: var(--danger);
  }
</style>
