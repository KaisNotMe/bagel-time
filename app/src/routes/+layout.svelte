<script lang="ts">
  import "../app.css";
  import { onMount, type Snippet } from "svelte";
  import { page } from "$app/state";
  import BagelLogo from "$lib/components/BagelLogo.svelte";
  import { games } from "$lib/games.svelte";

  let { children }: { children: Snippet } = $props();

  onMount(() => {
    games.listen();
  });

  const nav = [
    { href: "/", label: "Library", icon: "M3 4.5h4v4H3zM9 4.5h4v4H9zM3 10.5h4v4H3zM9 10.5h4v4H9z" },
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
        <a href={item.href} class:active={page.url.pathname === item.href}>
          <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
            <path d={item.icon} />
          </svg>
          {item.label}
        </a>
      {/each}
    </nav>
    <p class="foot">v0.1.0 · offline mode</p>
  </aside>
  <main>
    {@render children()}
  </main>
</div>

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
