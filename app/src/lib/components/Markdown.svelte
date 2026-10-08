<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { renderMarkdown } from "$lib/markdown";

  let { source }: { source: string } = $props();
  let html = $derived(renderMarkdown(source));

  // Links open in the real browser, not inside the launcher.
  function onclick(e: MouseEvent) {
    const link = (e.target as HTMLElement).closest("a[href]") as HTMLAnchorElement | null;
    if (!link) return;
    const href = link.getAttribute("href") ?? "";
    if (/^https?:\/\//i.test(href)) {
      e.preventDefault();
      openUrl(href);
    } else if (!href.startsWith("#")) {
      e.preventDefault();
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="markdown" {onclick}>{@html html}</div>

<style>
  .markdown {
    line-height: 1.65;
    color: var(--text);
    overflow-wrap: anywhere;
  }
  .markdown :global(h1),
  .markdown :global(h2),
  .markdown :global(h3),
  .markdown :global(h4) {
    margin: 1.2em 0 0.5em;
    line-height: 1.3;
  }
  .markdown :global(h1) {
    font-size: 22px;
  }
  .markdown :global(h2) {
    font-size: 19px;
  }
  .markdown :global(h3) {
    font-size: 16px;
  }
  .markdown :global(:first-child) {
    margin-top: 0;
  }
  .markdown :global(p) {
    margin: 0 0 0.9em;
  }
  .markdown :global(a) {
    color: var(--accent);
  }
  .markdown :global(img) {
    max-width: 100%;
    height: auto;
    border-radius: 8px;
    vertical-align: middle;
  }
  .markdown :global(iframe) {
    max-width: 100%;
    border: 0;
    border-radius: 10px;
  }
  .markdown :global(code) {
    font-family: var(--mono);
    font-size: 0.9em;
    padding: 1px 5px;
    border-radius: 5px;
    background: var(--raised-2);
  }
  .markdown :global(pre) {
    padding: 12px;
    border-radius: 10px;
    background: var(--rail);
    overflow: auto;
  }
  .markdown :global(pre code) {
    padding: 0;
    background: none;
  }
  .markdown :global(blockquote) {
    margin: 0 0 0.9em;
    padding: 4px 14px;
    border-left: 3px solid var(--line-strong);
    color: var(--muted);
  }
  .markdown :global(ul),
  .markdown :global(ol) {
    padding-left: 22px;
    margin: 0 0 0.9em;
  }
  .markdown :global(hr) {
    border: 0;
    border-top: 1px solid var(--line);
    margin: 1.4em 0;
  }
  .markdown :global(table) {
    border-collapse: collapse;
    margin: 0 0 1em;
  }
  .markdown :global(th),
  .markdown :global(td) {
    padding: 6px 10px;
    border: 1px solid var(--line);
  }
  .markdown :global(details) {
    margin: 0 0 0.9em;
    padding: 8px 12px;
    border-radius: 10px;
    background: var(--raised-2);
  }
  .markdown :global(summary) {
    cursor: pointer;
    font-weight: 600;
  }
</style>
