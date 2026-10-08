<script lang="ts">
  import { onMount } from "svelte";
  import { api, LOADER_NAMES, type ProjectType, type SearchHit } from "$lib/api";
  import BagelLogo from "$lib/components/BagelLogo.svelte";
  import Icon from "$lib/components/Icon.svelte";
  import InstanceIcon from "$lib/components/InstanceIcon.svelte";
  import ProjectIcon from "$lib/components/ProjectIcon.svelte";
  import { account } from "$lib/account.svelte";
  import { compactNumber, relativeTime } from "$lib/format";
  import { games } from "$lib/games.svelte";
  import { instances } from "$lib/instances.svelte";
  import { ui } from "$lib/ui.svelte";

  ui.setCrumbs({ label: "Home" });

  let packs = $state<SearchHit[]>([]);
  let mods = $state<SearchHit[]>([]);
  let offline = $state(false);

  async function popular(projectType: ProjectType, limit: number) {
    const r = await api.searchProjects({
      source: "modrinth",
      text: "",
      projectType,
      gameVersion: null,
      loader: null,
      categories: [],
      sort: "follows",
      offset: 0,
      limit,
    });
    return r.hits;
  }

  onMount(async () => {
    try {
      [packs, mods] = await Promise.all([popular("modpack", 6), popular("mod", 8)]);
    } catch {
      offline = true;
    }
  });

  let recent = $derived(instances.list.slice(0, 6));
  let greeting = $derived(account.activeName ? `Welcome back, ${account.activeName}` : "Welcome back");
</script>

<div class="page">
  <div class="page-inner">
    <h1 class="page-title">{greeting}</h1>

    <section>
      <div class="section-head">
        <h2 class="section-title">Jump back in</h2>
        <a class="more" href="/library">Library <Icon name="chevron-right" size={15} /></a>
      </div>
      {#if instances.loaded && recent.length === 0}
        <div class="welcome card">
          <BagelLogo size={56} />
          <div>
            <h3>Let's get baking</h3>
            <p class="muted">Create your first instance or grab a modpack to start playing.</p>
          </div>
          <div class="welcome-actions">
            <button class="btn primary" onclick={() => (ui.creatingInstance = true)}>
              <Icon name="plus" size={16} /> New instance
            </button>
            <a class="btn" href="/discover?type=modpack">Browse modpacks</a>
          </div>
        </div>
      {:else}
        <div class="recent">
          {#each recent as inst (inst.id)}
            {@const status = games.status[inst.id]}
            <div class="recent-card">
              <a class="recent-link" href="/instance?id={encodeURIComponent(inst.id)}">
                <InstanceIcon instance={inst} size={52} />
                <span class="recent-text">
                  <b>{inst.name}</b>
                  <small>{LOADER_NAMES[inst.loader]} {inst.gameVersion}</small>
                  <small class="faint">
                    {#if status?.phase === "running"}Playing now{:else if status}Starting…{:else}{relativeTime(inst.lastPlayed)}{/if}
                  </small>
                </span>
              </a>
              {#if status?.phase === "running"}
                <button class="btn danger-soft square" aria-label="Stop {inst.name}" onclick={() => games.stop(inst.id)}>
                  <Icon name="stop" size={14} fill />
                </button>
              {:else}
                <button
                  class="btn primary square"
                  aria-label="Play {inst.name}"
                  disabled={!!status}
                  onclick={() => games.launch(inst.id)}
                >
                  <Icon name="play" size={15} fill />
                </button>
              {/if}
            </div>
          {/each}
        </div>
      {/if}
    </section>

    {#if offline}
      <p class="muted">Couldn't reach Modrinth. Popular modpacks and mods show up here when you're online.</p>
    {:else}
      <section>
        <div class="section-head">
          <h2 class="section-title">Popular modpacks</h2>
          <a class="more" href="/discover?type=modpack">Discover modpacks <Icon name="chevron-right" size={15} /></a>
        </div>
        <div class="tiles">
          {#each packs as hit (hit.projectId)}
            <a class="tile" href="/project?id={hit.slug}">
              <div class="tile-head">
                <ProjectIcon url={hit.iconUrl} name={hit.title} size={48} />
                <div class="tile-title">
                  <b>{hit.title}</b>
                  <small>by {hit.author}</small>
                </div>
              </div>
              <p>{hit.description}</p>
              <span class="stats">
                <Icon name="download" size={13} />
                {compactNumber(hit.downloads)}
                <Icon name="heart" size={13} />
                {compactNumber(hit.follows)}
              </span>
            </a>
          {:else}
            {#each Array(6) as _, i (i)}<div class="tile skeleton"></div>{/each}
          {/each}
        </div>
      </section>

      <section>
        <div class="section-head">
          <h2 class="section-title">Popular mods</h2>
          <a class="more" href="/discover?type=mod">Discover mods <Icon name="chevron-right" size={15} /></a>
        </div>
        <div class="tiles small">
          {#each mods as hit (hit.projectId)}
            <a class="tile row" href="/project?id={hit.slug}">
              <ProjectIcon url={hit.iconUrl} name={hit.title} size={40} />
              <div class="tile-title">
                <b>{hit.title}</b>
                <small>{compactNumber(hit.downloads)} downloads</small>
              </div>
            </a>
          {:else}
            {#each Array(8) as _, i (i)}<div class="tile row skeleton"></div>{/each}
          {/each}
        </div>
      </section>
    {/if}
  </div>
</div>

<style>
  section {
    display: grid;
    gap: 12px;
  }
  .section-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .more {
    display: flex;
    align-items: center;
    gap: 2px;
    color: var(--muted);
    text-decoration: none;
    font-weight: 600;
    font-size: 13px;
  }
  .more:hover {
    color: var(--accent);
  }
  .welcome {
    display: flex;
    align-items: center;
    gap: 18px;
    padding: 20px;
  }
  .welcome h3 {
    font-size: 17px;
  }
  .welcome-actions {
    margin-left: auto;
    display: flex;
    gap: 8px;
  }
  .recent {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 12px;
  }
  .recent-card {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px;
    border-radius: var(--radius);
    background: var(--raised);
  }
  .recent-card:hover {
    background: var(--raised-2);
  }
  .recent-link {
    display: flex;
    align-items: center;
    gap: 12px;
    flex: 1;
    min-width: 0;
    color: inherit;
    text-decoration: none;
  }
  .recent-text {
    display: grid;
    min-width: 0;
    line-height: 1.3;
  }
  .recent-text b,
  .tile-title b {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .recent-text small {
    color: var(--muted);
    font-size: 12.5px;
  }
  .recent-text .faint {
    color: var(--faint);
  }
  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 12px;
  }
  .tiles.small {
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
  }
  .tile {
    display: grid;
    gap: 10px;
    align-content: start;
    padding: 14px;
    border-radius: var(--radius);
    background: var(--raised);
    color: inherit;
    text-decoration: none;
    min-width: 0;
    transition: background 0.12s;
  }
  .tile:hover {
    background: var(--raised-2);
  }
  .tile.row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
  }
  .tile-head {
    display: flex;
    gap: 12px;
    align-items: center;
    min-width: 0;
  }
  .tile-title {
    display: grid;
    min-width: 0;
    line-height: 1.3;
  }
  .tile-title small {
    color: var(--faint);
    font-size: 12.5px;
  }
  .tile p {
    color: var(--muted);
    font-size: 13px;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .stats {
    display: flex;
    align-items: center;
    gap: 5px;
    color: var(--faint);
    font-size: 12px;
  }
  .stats :global(svg:nth-of-type(2)) {
    margin-left: 8px;
  }
  .skeleton {
    min-height: 132px;
    animation: pulse 1.4s ease-in-out infinite;
  }
  .tile.row.skeleton {
    min-height: 60px;
  }
  @keyframes pulse {
    50% {
      opacity: 0.5;
    }
  }
</style>
