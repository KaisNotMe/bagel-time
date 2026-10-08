<script lang="ts">
  import { goto } from "$app/navigation";
  import { page } from "$app/state";
  import {
    api,
    errorMessage,
    LOADER_NAMES,
    PROJECT_TYPE_NAMES,
    supportedKinds,
    type Category,
    type Loader,
    type ProjectType,
    type SearchHit,
    type SortBy,
    type VersionInfo,
  } from "$lib/api";
  import Icon from "$lib/components/Icon.svelte";
  import InstallButton from "$lib/components/InstallButton.svelte";
  import InstanceIcon from "$lib/components/InstanceIcon.svelte";
  import ProjectIcon from "$lib/components/ProjectIcon.svelte";
  import { compactNumber } from "$lib/format";
  import { instances } from "$lib/instances.svelte";
  import { safeSvg } from "$lib/markdown";
  import { ui } from "$lib/ui.svelte";

  const PAGE = 20;
  const ALL_TYPES: ProjectType[] = ["mod", "resourcepack", "shader", "modpack"];
  const TYPE_ICONS = { mod: "puzzle", resourcepack: "image", shader: "sun", modpack: "box" } as const;
  const sorts: { value: SortBy; label: string }[] = [
    { value: "relevance", label: "Relevance" },
    { value: "downloads", label: "Downloads" },
    { value: "follows", label: "Follows" },
    { value: "updated", label: "Recently updated" },
    { value: "newest", label: "Newest" },
  ];

  let instanceId = $derived(page.url.searchParams.get("instance"));
  let target = $derived(instanceId ? instances.get(instanceId) : undefined);
  let types = $derived<ProjectType[]>(target ? supportedKinds(target.loader) : ALL_TYPES);
  let type = $derived.by<ProjectType>(() => {
    const t = page.url.searchParams.get("type") as ProjectType | null;
    return t && types.includes(t) ? t : types[0];
  });

  let text = $state("");
  let sort = $state<SortBy>("relevance");
  let gameVersion = $state<string | null>(null);
  let loader = $state<Loader | null>(null);
  let selectedCategories = $state<string[]>([]);
  let pageIndex = $state(0);

  let hits = $state<SearchHit[]>([]);
  let total = $state(0);
  let loading = $state(false);
  let error = $state("");

  let versions = $state<VersionInfo[]>([]);
  let categories = $state<Category[]>([]);
  let installed = $state<Set<string>>(new Set());
  let scroller = $state<HTMLDivElement>();

  // Filters are fixed to the instance when installing into one.
  let effectiveVersion = $derived(target ? target.gameVersion : gameVersion);
  let effectiveLoader = $derived<Loader | null>(
    type === "mod" || type === "modpack" ? (target ? target.loader : loader) : null,
  );
  let pages = $derived(Math.max(1, Math.ceil(Math.min(total, 10_000) / PAGE)));
  let groups = $derived.by(() => {
    const map = new Map<string, Category[]>();
    for (const c of categories.filter((c) => c.projectType === type)) {
      map.set(c.header, [...(map.get(c.header) ?? []), c]);
    }
    return [...map.entries()].map(([header, items]) => ({
      header,
      items: items.sort((a, b) => a.name.localeCompare(b.name)),
    }));
  });

  $effect(() => {
    ui.setCrumbs(
      { label: "Discover", href: "/discover" },
      { label: target ? `${PROJECT_TYPE_NAMES[type].many} for ${target.name}` : PROJECT_TYPE_NAMES[type].many },
    );
  });

  // Lists for the filters, once.
  $effect(() => {
    api.listVersions(false).then((v) => (versions = v)).catch(() => {});
    api.getCategories().then((c) => (categories = c)).catch(() => {});
  });

  async function loadInstalled() {
    if (!instanceId) {
      installed = new Set();
      return;
    }
    try {
      const list = await api.listContent(instanceId);
      installed = new Set(list.map((c) => c.projectId).filter((p): p is string => !!p));
    } catch {
      installed = new Set();
    }
  }
  $effect(() => {
    void instanceId;
    loadInstalled();
  });

  // New type: categories from another type don't apply.
  $effect(() => {
    void type;
    selectedCategories = [];
  });

  // Back to the first page whenever the search changes.
  $effect(() => {
    void [text, sort, type, effectiveVersion, effectiveLoader, selectedCategories.length];
    pageIndex = 0;
  });

  let request = 0;
  $effect(() => {
    const args = {
      text,
      projectType: type,
      gameVersion: effectiveVersion,
      loader: effectiveLoader,
      categories: [...selectedCategories],
      sort,
      offset: pageIndex * PAGE,
      limit: PAGE,
    };
    const id = ++request;
    const timer = setTimeout(async () => {
      loading = true;
      error = "";
      try {
        const r = await api.searchModrinth(args);
        if (id !== request) return;
        hits = r.hits;
        total = r.totalHits;
        scroller?.scrollTo({ top: 0 });
      } catch (e) {
        if (id === request) error = `Couldn't reach Modrinth: ${errorMessage(e)}`;
      } finally {
        if (id === request) loading = false;
      }
    }, 250);
    return () => clearTimeout(timer);
  });

  function setType(t: ProjectType) {
    const url = new URL(page.url);
    url.searchParams.set("type", t);
    goto(url, { replaceState: true, keepFocus: true, noScroll: true });
  }

  function clearTarget() {
    const url = new URL(page.url);
    url.searchParams.delete("instance");
    goto(url, { replaceState: true });
  }

  function toggleCategory(name: string) {
    selectedCategories = selectedCategories.includes(name)
      ? selectedCategories.filter((c) => c !== name)
      : [...selectedCategories, name];
  }

  function projectHref(hit: SearchHit) {
    return `/project?id=${encodeURIComponent(hit.slug)}${instanceId ? `&instance=${encodeURIComponent(instanceId)}` : ""}`;
  }
</script>

<div class="page" bind:this={scroller}>
  <div class="page-inner">
    <div class="head">
      <h1 class="page-title">Discover</h1>
      <nav class="pills" aria-label="Project type">
        {#each types as t (t)}
          <button class="pill" class:active={type === t} onclick={() => setType(t)}>
            <Icon name={TYPE_ICONS[t]} size={15} />
            {PROJECT_TYPE_NAMES[t].many}
          </button>
        {/each}
      </nav>
    </div>

    {#if target}
      <div class="target card">
        <InstanceIcon instance={target} size={36} />
        <div class="target-text">
          <small>Installing to</small>
          <a href="/instance?id={encodeURIComponent(target.id)}"><b>{target.name}</b></a>
        </div>
        <span class="tag">{LOADER_NAMES[target.loader]} {target.gameVersion}</span>
        <button class="btn ghost sm" onclick={clearTarget}><Icon name="x" size={14} /> Browse everything</button>
      </div>
    {/if}

    <div class="layout">
      <aside class="filters">
        <section>
          <h3>Minecraft version</h3>
          {#if target}
            <p class="locked"><Icon name="info" size={14} /> {target.gameVersion}, from the instance</p>
          {:else}
            <select class="input" bind:value={gameVersion}>
              <option value={null}>All versions</option>
              {#each versions as v (v.id)}<option value={v.id}>{v.id}</option>{/each}
            </select>
          {/if}
        </section>

        {#if type === "mod" || type === "modpack"}
          <section>
            <h3>Loader</h3>
            {#if target}
              <p class="locked"><Icon name="info" size={14} /> {LOADER_NAMES[target.loader]}, from the instance</p>
            {:else}
              <div class="pills">
                <button class="pill small" class:active={loader === null} onclick={() => (loader = null)}>Any</button>
                {#each ["fabric", "quilt", "forge", "neoforge"] as l (l)}
                  <button class="pill small" class:active={loader === l} onclick={() => (loader = l as Loader)}>
                    {LOADER_NAMES[l as Loader]}
                  </button>
                {/each}
              </div>
            {/if}
          </section>
        {/if}

        {#each groups as g (g.header)}
          <section>
            <h3>{g.header}</h3>
            <div class="cats">
              {#each g.items as c (c.name)}
                <label class="cat" class:on={selectedCategories.includes(c.name)}>
                  <input type="checkbox" checked={selectedCategories.includes(c.name)} onchange={() => toggleCategory(c.name)} />
                  <span class="cat-icon">{@html safeSvg(c.icon)}</span>
                  <span class="cat-name">{c.name.replace(/-/g, " ")}</span>
                </label>
              {/each}
            </div>
          </section>
        {/each}

        {#if selectedCategories.length > 0 || gameVersion || loader}
          <button
            class="btn ghost sm"
            onclick={() => {
              selectedCategories = [];
              gameVersion = null;
              loader = null;
            }}><Icon name="x" size={14} /> Clear filters</button
          >
        {/if}
      </aside>

      <div class="results">
        <div class="toolbar">
          <label class="search-box">
            <Icon name="search" size={16} />
            <input class="input" type="search" placeholder="Search {PROJECT_TYPE_NAMES[type].many.toLowerCase()}" bind:value={text} />
          </label>
          <label class="sort">
            <span>Sort</span>
            <select class="input" bind:value={sort}>
              {#each sorts as s (s.value)}<option value={s.value}>{s.label}</option>{/each}
            </select>
          </label>
        </div>

        {#if error}<p class="alert">{error}</p>{/if}
        <p class="count faint">
          {#if loading}Searching…{:else}{total.toLocaleString()} {total === 1 ? "result" : "results"}{/if}
        </p>

        <div class="list" class:dim={loading}>
          {#each hits as hit (hit.projectId)}
            <article class="hit">
              <a class="hit-icon" href={projectHref(hit)} tabindex="-1" aria-hidden="true">
                <ProjectIcon url={hit.iconUrl} name={hit.title} size={72} />
              </a>
              <div class="body">
                <div class="title">
                  <a href={projectHref(hit)}><h3>{hit.title}</h3></a>
                  <span class="author">by {hit.author}</span>
                </div>
                <p class="desc">{hit.description}</p>
                <div class="tags">
                  {#each hit.displayCategories.slice(0, 4) as tag (tag)}
                    <span class="tag">{tag.replace(/-/g, " ")}</span>
                  {/each}
                </div>
              </div>
              <div class="side">
                <div class="stats">
                  <span title="{hit.downloads.toLocaleString()} downloads"><Icon name="download" size={14} /> {compactNumber(hit.downloads)}</span>
                  <span title="{hit.follows.toLocaleString()} followers"><Icon name="heart" size={14} /> {compactNumber(hit.follows)}</span>
                </div>
                <InstallButton
                  projectId={hit.projectId}
                  title={hit.title}
                  projectType={type}
                  instanceId={target?.id ?? null}
                  installed={installed.has(hit.projectId)}
                  oninstalled={loadInstalled}
                />
              </div>
            </article>
          {:else}
            {#if !loading && !error}
              <div class="empty-state">
                <Icon name="search" size={32} stroke={1.4} />
                <h3>Nothing found</h3>
                <p>Try different words or fewer filters.</p>
              </div>
            {/if}
          {/each}
        </div>

        {#if pages > 1}
          <nav class="pager" aria-label="Pages">
            <button class="btn sm" disabled={pageIndex === 0 || loading} onclick={() => pageIndex--}>
              <Icon name="chevron-left" size={15} /> Previous
            </button>
            <span class="faint">Page {pageIndex + 1} of {pages.toLocaleString()}</span>
            <button class="btn sm" disabled={pageIndex + 1 >= pages || loading} onclick={() => pageIndex++}>
              Next <Icon name="chevron-right" size={15} />
            </button>
          </nav>
        {/if}
      </div>
    </div>
  </div>
</div>

<style>
  .head {
    display: flex;
    align-items: center;
    gap: 20px;
    flex-wrap: wrap;
  }
  .target {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    background: var(--accent-soft);
  }
  .target-text {
    display: grid;
    line-height: 1.25;
    flex: 1;
    min-width: 0;
  }
  .target-text small {
    color: var(--muted);
  }
  .target-text a {
    color: var(--text);
    text-decoration: none;
  }
  .layout {
    display: grid;
    grid-template-columns: 240px minmax(0, 1fr);
    gap: 24px;
    align-items: start;
  }
  .filters {
    display: grid;
    gap: 18px;
  }
  .filters section {
    display: grid;
    gap: 8px;
  }
  .filters h3 {
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--faint);
  }
  .locked {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--muted);
    font-size: 13px;
  }
  .pill.small {
    height: 28px;
    padding: 0 12px;
    font-size: 13px;
  }
  .cats {
    display: grid;
    gap: 1px;
  }
  .cat {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 5px 8px;
    border-radius: 8px;
    color: var(--muted);
    cursor: pointer;
    text-transform: capitalize;
  }
  .cat:hover {
    background: var(--raised);
    color: var(--text);
  }
  .cat.on {
    color: var(--accent);
    background: var(--accent-soft);
  }
  .cat input {
    position: absolute;
    opacity: 0;
    width: 0;
    height: 0;
  }
  .cat:has(input:focus-visible) {
    outline: 2px solid var(--accent);
  }
  .cat-icon {
    display: grid;
    place-items: center;
    width: 16px;
    height: 16px;
  }
  .cat-icon :global(svg) {
    width: 16px;
    height: 16px;
  }
  .results {
    display: grid;
    gap: 12px;
    min-width: 0;
  }
  .toolbar {
    display: flex;
    gap: 10px;
  }
  .sort {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--muted);
    font-weight: 600;
    font-size: 13px;
  }
  .count {
    font-size: 13px;
  }
  .list {
    display: grid;
    gap: 10px;
    transition: opacity 0.15s;
  }
  .list.dim {
    opacity: 0.6;
  }
  .hit {
    display: flex;
    gap: 16px;
    padding: 14px;
    border-radius: var(--radius);
    background: var(--raised);
  }
  .hit:hover {
    background: var(--raised-2);
  }
  .body {
    flex: 1;
    min-width: 0;
    display: grid;
    align-content: start;
    gap: 5px;
  }
  .title {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
  }
  .title a {
    color: var(--text);
    text-decoration: none;
    min-width: 0;
  }
  .title a:hover h3 {
    color: var(--accent);
  }
  h3 {
    font-size: 16px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .author {
    color: var(--faint);
    font-size: 13px;
    white-space: nowrap;
  }
  .desc {
    color: var(--muted);
    font-size: 13.5px;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-top: 2px;
  }
  .tags .tag {
    text-transform: capitalize;
  }
  .side {
    display: grid;
    justify-items: end;
    align-content: space-between;
    gap: 10px;
  }
  .stats {
    display: grid;
    justify-items: end;
    gap: 2px;
    color: var(--muted);
    font-size: 13px;
    font-weight: 600;
  }
  .stats span {
    display: flex;
    align-items: center;
    gap: 5px;
  }
  .pager {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 14px;
    padding-top: 6px;
  }
</style>
