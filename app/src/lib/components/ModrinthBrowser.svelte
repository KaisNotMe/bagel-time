<script lang="ts">
  import type { Snippet } from "svelte";
  import { api, errorMessage, type Loader, type ProjectType, type SearchHit, type SortBy } from "$lib/api";
  import { compactNumber } from "$lib/format";
  import ProjectIcon from "./ProjectIcon.svelte";

  type Props = {
    projectType: ProjectType;
    gameVersion?: string | null;
    loader?: Loader | null;
    placeholder?: string;
    /** The button (or label) shown on each result. */
    action: Snippet<[SearchHit]>;
  };
  let { projectType, gameVersion = null, loader = null, placeholder = "Search Modrinth", action }: Props =
    $props();

  const PAGE = 20;
  const sorts: { value: SortBy; label: string }[] = [
    { value: "relevance", label: "Relevance" },
    { value: "downloads", label: "Downloads" },
    { value: "follows", label: "Follows" },
    { value: "updated", label: "Recently updated" },
    { value: "newest", label: "Newest" },
  ];

  let text = $state("");
  let sort = $state<SortBy>("relevance");
  let hits = $state<SearchHit[]>([]);
  let total = $state(0);
  let loading = $state(false);
  let error = $state("");

  // Ignore responses that arrive after a newer search started.
  let request = 0;
  async function search(append: boolean) {
    const id = ++request;
    loading = true;
    error = "";
    try {
      const results = await api.searchModrinth({
        text,
        projectType,
        gameVersion,
        loader,
        sort,
        offset: append ? hits.length : 0,
        limit: PAGE,
      });
      if (id !== request) return;
      hits = append ? [...hits, ...results.hits] : results.hits;
      total = results.totalHits;
    } catch (e) {
      if (id === request) error = `Couldn't reach Modrinth: ${errorMessage(e)}`;
    } finally {
      if (id === request) loading = false;
    }
  }

  // Search again (debounced) whenever the query or filters change.
  $effect(() => {
    void [text, sort, projectType, gameVersion, loader];
    const timer = setTimeout(() => search(false), 300);
    return () => clearTimeout(timer);
  });
</script>

<div class="browser">
  <div class="toolbar">
    <div class="search">
      <svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round">
        <circle cx="7" cy="7" r="4.5" /><path d="M10.5 10.5 14 14" />
      </svg>
      <input class="input" type="search" bind:value={text} {placeholder} aria-label={placeholder} />
    </div>
    <label class="sort">
      <span>Sort</span>
      <select class="input" bind:value={sort}>
        {#each sorts as s (s.value)}
          <option value={s.value}>{s.label}</option>
        {/each}
      </select>
    </label>
  </div>

  {#if error}
    <p class="alert">{error}</p>
  {/if}

  {#if !loading && !error && hits.length === 0}
    <p class="empty">Nothing found{text ? ` for “${text}”` : ""}.</p>
  {/if}

  <ul class="results">
    {#each hits as hit (hit.projectId)}
      <li class="hit">
        <ProjectIcon url={hit.iconUrl} name={hit.title} size={52} />
        <div class="body">
          <div class="title">
            <h3>{hit.title}</h3>
            <span class="author">by {hit.author}</span>
          </div>
          <p class="desc">{hit.description}</p>
          <div class="stats">
            <span title="{hit.downloads.toLocaleString()} downloads">
              <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                <path d="M8 2v9M4 7l4 4 4-4M3 14h10" />
              </svg>
              {compactNumber(hit.downloads)}
            </span>
            <span title="{hit.follows.toLocaleString()} followers">
              <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round">
                <path d="M8 13.5S2.5 10 2.5 6.2A2.8 2.8 0 0 1 8 5a2.8 2.8 0 0 1 5.5 1.2C13.5 10 8 13.5 8 13.5z" />
              </svg>
              {compactNumber(hit.follows)}
            </span>
            {#each hit.displayCategories.slice(0, 3) as tag (tag)}
              <span class="tag">{tag}</span>
            {/each}
          </div>
        </div>
        <div class="action">{@render action(hit)}</div>
      </li>
    {/each}
  </ul>

  {#if loading}
    <p class="empty">Searching…</p>
  {:else if hits.length < total}
    <button class="btn more" onclick={() => search(true)}>Load more</button>
  {/if}
</div>

<style>
  .browser {
    display: grid;
    gap: 14px;
  }
  .toolbar {
    display: flex;
    gap: 10px;
  }
  .search {
    position: relative;
    flex: 1;
  }
  .search svg {
    position: absolute;
    left: 11px;
    top: 50%;
    transform: translateY(-50%);
    color: var(--faint);
    pointer-events: none;
  }
  .search .input {
    width: 100%;
    padding-left: 33px;
  }
  .sort {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--muted);
  }
  .results {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 8px;
  }
  .hit {
    display: flex;
    gap: 14px;
    align-items: center;
    padding: 12px 14px;
    border: 1px solid var(--line);
    border-radius: var(--radius);
    background: var(--card);
  }
  .hit:hover {
    background: var(--card-hover);
  }
  .body {
    flex: 1;
    min-width: 0;
    display: grid;
    gap: 3px;
  }
  .title {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
  }
  h3 {
    font-size: 15px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .author {
    color: var(--faint);
    font-size: 12.5px;
    white-space: nowrap;
  }
  .desc {
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
    flex-wrap: wrap;
    align-items: center;
    gap: 12px;
    margin-top: 2px;
    color: var(--faint);
    font-size: 12px;
  }
  .stats > span {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .tag {
    padding: 1px 7px;
    border-radius: 999px;
    background: var(--card-hover);
    border: 1px solid var(--line);
    text-transform: capitalize;
  }
  .action {
    flex: none;
  }
  .empty {
    color: var(--faint);
    text-align: center;
    padding: 18px 0;
  }
  .more {
    justify-self: center;
  }
</style>
