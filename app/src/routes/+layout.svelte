<script lang="ts">
  import "../app.css";
  import { onMount, type Snippet } from "svelte";
  import { page } from "$app/state";
  import BagelLogo from "$lib/components/BagelLogo.svelte";
  import { account } from "$lib/account.svelte";
  import { games } from "$lib/games.svelte";
  import { packs } from "$lib/packs.svelte";

  let { children }: { children: Snippet } = $props();

  onMount(() => {
    games.listen();
    packs.listen();
    account.refresh();
  });

  /** Instance pages belong to the Library. */
  function isActive(href: string, path: string) {
    if (href === "/") return path === "/" || path.startsWith("/instance");
    return path.startsWith(href);
  }

  const nav = [
    { href: "/", label: "Library", icon: "M3 4.5h4v4H3zM9 4.5h4v4H9zM3 10.5h4v4H3zM9 10.5h4v4H9z" },
    {
      href: "/modpacks",
      label: "Modpacks",
      icon: "M8 1.8 13.5 4.6v6.8L8 14.2 2.5 11.4V4.6zM2.5 4.6 8 7.4l5.5-2.8M8 7.4v6.8",
    },
    {
      href: "/settings",
      label: "Settings",
      icon: "M8 5.5a2.5 2.5 0 1 0 0 5 2.5 2.5 0 0 0 0-5zM8 1.5v2M8 12.5v2M1.5 8h2M12.5 8h2M3.4 3.4l1.4 1.4M11.2 11.2l1.4 1.4M3.4 12.6l1.4-1.4M11.2 4.8l1.4-1.4",
    },
  ];
</script>

<div class="shell">
  <aside>
    <div class="brand">
      <BagelLogo size={30} />
      <span>Bagel Time</span>
    </div>
    <nav>
      {#each nav as item (item.href)}
        <a href={item.href} class:active={isActive(item.href, page.url.pathname)}>
          <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
            <path d={item.icon} />
          </svg>
          {item.label}
        </a>
      {/each}
    </nav>
    <a class="foot" href="/settings">
      {#if account.activeName}
        Playing as <b>{account.activeName}</b>
      {:else}
        Offline mode
      {/if}
    </a>
  </aside>
  <main>
    {@render children()}
  </main>
</div>

{#if packs.dragging}
  <div class="drop" aria-hidden="true">
    <div>Drop a .mrpack file to create an instance</div>
  </div>
{/if}

{#if packs.status || packs.error}
  <div class="toast" role="status" aria-live="polite">
    {#if packs.status}
      {@const pct = packs.status.total > 0 ? Math.round((packs.status.done / packs.status.total) * 100) : null}
      <div class="toast-head">
        <b>Installing {packs.status.name}</b>
        {#if pct !== null}<span>{pct}%</span>{/if}
      </div>
      <p>{packs.status.stage}</p>
      <div class="bar"><div class="fill" class:indeterminate={pct === null} style:width="{pct ?? 35}%"></div></div>
    {:else}
      <div class="toast-head">
        <b class="bad">Modpack not installed</b>
        <button class="x" aria-label="Dismiss" onclick={() => (packs.error = "")}>×</button>
      </div>
      <p>{packs.error}</p>
    {/if}
  </div>
{/if}

<style>
  .shell {
    display: grid;
    grid-template-columns: 220px 1fr;
    height: 100vh;
  }
  aside {
    display: flex;
    flex-direction: column;
    gap: 22px;
    padding: 18px 12px;
    background: var(--panel);
    border-right: 1px solid var(--line);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 8px;
    font-weight: 800;
    font-size: 17px;
    letter-spacing: 0.2px;
  }
  nav {
    display: grid;
    gap: 2px;
  }
  nav a {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 10px;
    border-radius: 8px;
    color: var(--muted);
    text-decoration: none;
    font-weight: 600;
  }
  nav a:hover {
    background: var(--card);
    color: var(--text);
  }
  nav a.active {
    background: var(--card-hover);
    color: var(--text);
    box-shadow: inset 3px 0 0 var(--accent);
  }
  .foot {
    margin-top: auto;
    padding: 0 10px;
    color: var(--faint);
    font-size: 12px;
    text-decoration: none;
  }
  .foot:hover {
    color: var(--muted);
  }
  .foot b {
    color: var(--text);
  }
  .drop {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: grid;
    place-items: center;
    padding: 24px;
    background: rgb(8 6 5 / 0.65);
    pointer-events: none;
  }
  .drop div {
    display: grid;
    place-items: center;
    width: 100%;
    height: 100%;
    border: 2px dashed var(--accent);
    border-radius: 18px;
    font-size: 18px;
    font-weight: 700;
    color: var(--accent);
  }
  .toast {
    position: fixed;
    right: 20px;
    bottom: 20px;
    z-index: 40;
    width: 320px;
    padding: 14px;
    display: grid;
    gap: 6px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radius);
    background: var(--panel);
    box-shadow: 0 16px 40px rgb(0 0 0 / 0.5);
  }
  .toast-head {
    display: flex;
    justify-content: space-between;
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
  .x {
    border: 0;
    background: none;
    color: var(--muted);
    font-size: 18px;
    line-height: 1;
    cursor: pointer;
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
  main {
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }
  @media (max-width: 900px) {
    .shell {
      grid-template-columns: 64px 1fr;
    }
    .brand span,
    .foot {
      display: none;
    }
    nav a {
      justify-content: center;
      font-size: 0;
      gap: 0;
    }
  }
</style>
