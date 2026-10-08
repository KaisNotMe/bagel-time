<script lang="ts">
  import "../app.css";
  import { onMount, type Snippet } from "svelte";
  import { goto } from "$app/navigation";
  import { page } from "$app/state";
  import BagelLogo from "$lib/components/BagelLogo.svelte";
  import Icon, { type IconName } from "$lib/components/Icon.svelte";
  import InstanceIcon from "$lib/components/InstanceIcon.svelte";
  import NewInstanceDialog from "$lib/components/NewInstanceDialog.svelte";
  import { api } from "$lib/api";
  import { account } from "$lib/account.svelte";
  import { games } from "$lib/games.svelte";
  import { instances } from "$lib/instances.svelte";
  import { packs } from "$lib/packs.svelte";
  import { ui } from "$lib/ui.svelte";

  let { children }: { children: Snippet } = $props();

  let showSnapshots = $state(false);

  onMount(() => {
    games.listen();
    packs.listen();
    account.refresh();
    instances.refresh();
    api.getSettings().then((s) => (showSnapshots = s.showSnapshots));
  });

  const nav: { href: string; label: string; icon: IconName; match: (p: string) => boolean }[] = [
    { href: "/", label: "Home", icon: "home", match: (p) => p === "/" },
    { href: "/library", label: "Library", icon: "library", match: (p) => p.startsWith("/library") || p.startsWith("/instance") },
    { href: "/discover", label: "Discover", icon: "compass", match: (p) => p.startsWith("/discover") || p.startsWith("/project") },
  ];

  let path = $derived(page.url.pathname);
  let currentInstance = $derived(path.startsWith("/instance") ? page.url.searchParams.get("id") : null);

  /** Instances that are starting or running, for the top bar. */
  let active = $derived(
    Object.entries(games.status).map(([id, status]) => ({ id, status, instance: instances.get(id) })),
  );

  function percent(done: number, total: number) {
    return total > 0 ? Math.min(100, Math.round((done / total) * 100)) : null;
  }
</script>

<div class="shell">
  <nav class="rail" aria-label="Main">
    <a class="logo" href="/" aria-label="Bagel Time home"><BagelLogo size={34} /></a>

    {#each nav as item (item.href)}
      <a class="rail-btn" class:active={item.match(path)} href={item.href} aria-label={item.label}>
        <Icon name={item.icon} size={21} />
        <span class="tip">{item.label}</span>
      </a>
    {/each}

    <div class="divider"></div>

    <div class="recent">
      {#each instances.recent as inst (inst.id)}
        <a
          class="rail-instance"
          class:active={currentInstance === inst.id}
          href="/instance?id={encodeURIComponent(inst.id)}"
          aria-label={inst.name}
        >
          <InstanceIcon instance={inst} size={38} radius={11} />
          {#if games.status[inst.id]}<span class="live" class:starting={games.status[inst.id].phase !== "running"}></span>{/if}
          <span class="tip">{inst.name}</span>
        </a>
      {/each}
      <button class="rail-btn add" aria-label="New instance" onclick={() => (ui.creatingInstance = true)}>
        <Icon name="plus" size={20} />
        <span class="tip">New instance</span>
      </button>
    </div>

    <a class="rail-btn bottom" class:active={path.startsWith("/settings")} href="/settings" aria-label="Settings">
      <Icon name="settings" size={21} />
      <span class="tip">Settings</span>
    </a>
  </nav>

  <header class="topbar">
    <div class="history">
      <button class="btn ghost sm square" aria-label="Back" onclick={() => history.back()}>
        <Icon name="chevron-left" size={18} />
      </button>
      <button class="btn ghost sm square" aria-label="Forward" onclick={() => history.forward()}>
        <Icon name="chevron-right" size={18} />
      </button>
    </div>
    <ol class="crumbs">
      {#each ui.crumbs as crumb, i (i)}
        <li>
          {#if i > 0}<Icon name="chevron-right" size={14} />{/if}
          {#if crumb.href && i < ui.crumbs.length - 1}
            <a href={crumb.href}>{crumb.label}</a>
          {:else}
            <span class:current={i === ui.crumbs.length - 1}>{crumb.label}</span>
          {/if}
        </li>
      {/each}
    </ol>

    <div class="right">
      {#if active.length === 0}
        <span class="idle"><span class="dot"></span>No instances running</span>
      {:else}
        {#each active as a (a.id)}
          {@const pct = percent(a.status.done, a.status.total)}
          <div class="running">
            <a class="running-link" href="/instance?id={encodeURIComponent(a.id)}">
              {#if a.instance}<InstanceIcon instance={a.instance} size={22} radius={6} />{/if}
              <span class="running-text">
                <b>{a.instance?.name ?? a.id}</b>
                <small>
                  {#if a.status.phase === "running"}
                    <span class="dot on"></span> Running
                  {:else}
                    {a.status.stage || "Starting"}{pct !== null ? ` · ${pct}%` : ""}
                  {/if}
                </small>
              </span>
            </a>
            <button
              class="btn ghost sm square"
              aria-label="Open log"
              title="Log"
              onclick={() => goto(`/instance?id=${encodeURIComponent(a.id)}&tab=logs`)}
            >
              <Icon name="terminal" size={16} />
            </button>
            {#if a.status.phase === "running"}
              <button class="btn danger-soft sm square" aria-label="Stop" title="Stop" onclick={() => games.stop(a.id)}>
                <Icon name="stop" size={14} fill />
              </button>
            {/if}
          </div>
        {/each}
      {/if}

      <a class="account" href="/settings" title="Accounts and settings">
        {#if account.list.active}
          <img src="https://mc-heads.net/avatar/{account.list.active}/24" alt="" width="24" height="24" />
          <span>{account.activeName}</span>
        {:else}
          <span class="offline"><Icon name="user" size={14} /></span>
          <span>Offline</span>
        {/if}
      </a>
    </div>
  </header>

  <main>
    {@render children()}
  </main>
</div>

<NewInstanceDialog
  open={ui.creatingInstance}
  {showSnapshots}
  onclose={() => (ui.creatingInstance = false)}
  oncreated={async (instance) => {
    ui.creatingInstance = false;
    await instances.refresh();
    goto(`/instance?id=${encodeURIComponent(instance.id)}`);
  }}
/>

{#if packs.dragging}
  <div class="drop" aria-hidden="true">
    <div>
      <Icon name="box" size={40} stroke={1.5} />
      Drop a .mrpack file to create an instance
    </div>
  </div>
{/if}

{#if packs.status || packs.error}
  <div class="toast" role="status" aria-live="polite">
    {#if packs.status}
      {@const pct = percent(packs.status.done, packs.status.total)}
      <div class="toast-head">
        <b>Installing {packs.status.name}</b>
        {#if pct !== null}<span>{pct}%</span>{/if}
      </div>
      <p>{packs.status.stage}</p>
      <div class="bar"><div class="fill" class:indeterminate={pct === null} style:width="{pct ?? 35}%"></div></div>
    {:else}
      <div class="toast-head">
        <b class="bad">Modpack not installed</b>
        <button class="btn ghost sm square" aria-label="Dismiss" onclick={() => (packs.error = "")}>
          <Icon name="x" size={15} />
        </button>
      </div>
      <p>{packs.error}</p>
    {/if}
  </div>
{/if}

<style>
  .shell {
    display: grid;
    grid-template-columns: 68px 1fr;
    grid-template-rows: 52px 1fr;
    height: 100vh;
    background: var(--rail);
  }

  /* Left rail */
  .rail {
    grid-row: 1 / 3;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 10px 0 12px;
    min-height: 0;
  }
  .logo {
    display: grid;
    place-items: center;
    width: 46px;
    height: 46px;
    margin-bottom: 6px;
    border-radius: 14px;
  }
  .rail-btn,
  .rail-instance {
    position: relative;
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    border: 0;
    border-radius: 12px;
    background: none;
    color: var(--muted);
    cursor: pointer;
    text-decoration: none;
    transition: background 0.12s, color 0.12s;
  }
  .rail-btn:hover {
    background: var(--raised);
    color: var(--text);
  }
  .rail-btn.active {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .rail-instance {
    border-radius: 13px;
    outline: 2px solid transparent;
    outline-offset: 1px;
  }
  .rail-instance:hover {
    outline-color: var(--line-strong);
  }
  .rail-instance.active {
    outline-color: var(--accent);
  }
  .rail-btn.add {
    border: 1.5px dashed var(--line-strong);
  }
  .rail-btn.add:hover {
    border-color: var(--accent);
    color: var(--accent);
    background: none;
  }
  .live {
    position: absolute;
    right: -1px;
    bottom: -1px;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--ok);
    border: 2.5px solid var(--rail);
  }
  .live.starting {
    background: var(--accent);
  }
  .divider {
    width: 28px;
    height: 1px;
    margin: 6px 0;
    background: var(--line);
  }
  .recent {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    min-height: 0;
    overflow: visible;
  }
  .bottom {
    margin-top: auto;
  }
  .tip {
    position: absolute;
    left: calc(100% + 12px);
    top: 50%;
    transform: translateY(-50%);
    z-index: 30;
    padding: 5px 10px;
    border-radius: 8px;
    background: var(--raised-3);
    color: var(--text);
    font-size: 13px;
    font-weight: 600;
    white-space: nowrap;
    pointer-events: none;
    opacity: 0;
    transition: opacity 0.12s;
    box-shadow: 0 6px 20px rgb(0 0 0 / 0.4);
  }
  .rail-btn:hover .tip,
  .rail-instance:hover .tip {
    opacity: 1;
  }

  /* Top bar */
  .topbar {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 0 14px 0 4px;
    min-width: 0;
  }
  .history {
    display: flex;
    gap: 2px;
    color: var(--muted);
  }
  .crumbs {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
    min-width: 0;
    flex: 1;
    color: var(--faint);
  }
  .crumbs li {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }
  .crumbs a {
    color: var(--muted);
    text-decoration: none;
    font-weight: 600;
  }
  .crumbs a:hover {
    color: var(--text);
  }
  .crumbs span {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .crumbs .current {
    color: var(--text);
  }
  .right {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }
  .idle {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--faint);
    font-size: 13px;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--faint);
    display: inline-block;
  }
  .dot.on {
    background: var(--ok);
  }
  .running {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 3px 4px 3px 6px;
    border-radius: 12px;
    background: var(--raised);
  }
  .running-link {
    display: flex;
    align-items: center;
    gap: 8px;
    color: inherit;
    text-decoration: none;
    min-width: 0;
    padding-right: 4px;
  }
  .running-text {
    display: grid;
    line-height: 1.2;
    min-width: 0;
  }
  .running-text b {
    font-size: 13px;
    max-width: 160px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .running-text small {
    display: flex;
    align-items: center;
    gap: 5px;
    color: var(--muted);
    font-size: 11.5px;
    max-width: 180px;
    overflow: hidden;
    white-space: nowrap;
  }
  .account {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 36px;
    padding: 0 12px 0 6px;
    border-radius: 12px;
    background: var(--raised);
    color: var(--text);
    text-decoration: none;
    font-weight: 600;
    font-size: 13px;
  }
  .account:hover {
    background: var(--raised-2);
  }
  .account img {
    border-radius: 6px;
    image-rendering: pixelated;
  }
  .offline {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border-radius: 6px;
    background: var(--raised-3);
    color: var(--muted);
  }

  /* Content area: a rounded surface inset from the rail and top bar. */
  main {
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    background: var(--bg);
    border-top-left-radius: var(--radius-lg);
    border-top: 1px solid var(--line);
    border-left: 1px solid var(--line);
  }

  .drop {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: grid;
    place-items: center;
    padding: 24px;
    background: rgb(8 6 5 / 0.7);
    pointer-events: none;
  }
  .drop div {
    display: grid;
    place-items: center;
    align-content: center;
    gap: 14px;
    width: 100%;
    height: 100%;
    border: 2px dashed var(--accent);
    border-radius: 22px;
    font-size: 18px;
    font-weight: 700;
    color: var(--accent);
  }
  .toast {
    position: fixed;
    right: 20px;
    bottom: 20px;
    z-index: 40;
    width: 330px;
    padding: 14px;
    display: grid;
    gap: 6px;
    border-radius: var(--radius);
    background: var(--raised-2);
    box-shadow: var(--shadow);
  }
  .toast-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
  }
  .toast-head b {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .toast p {
    color: var(--muted);
    font-size: 13px;
    word-break: break-word;
  }
  .bad {
    color: var(--danger);
  }
  .bar {
    height: 6px;
    border-radius: 3px;
    background: var(--raised-3);
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
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(300%);
    }
  }
</style>
