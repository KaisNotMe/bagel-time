<script lang="ts">
  import { tick } from "svelte";
  import { logTime } from "$lib/format";
  import { games } from "$lib/games.svelte";

  let { instanceId, name }: { instanceId: string; name: string } = $props();

  let lines = $derived(games.logs[instanceId] ?? []);
  let scroller = $state<HTMLDivElement>();
  let followTail = $state(true);
  let copied = $state(false);

  $effect(() => {
    void lines.length;
    if (followTail) {
      tick().then(() => {
        if (scroller) scroller.scrollTop = scroller.scrollHeight;
      });
    }
  });

  function onscroll() {
    if (!scroller) return;
    followTail = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 40;
  }

  async function copy() {
    const text = lines
      .map((l) => `[${logTime(l.timestamp)}] [${l.thread ?? "main"}/${l.level}] ${l.message}`)
      .join("\n");
    await navigator.clipboard.writeText(text);
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }
</script>

<section class="panel" aria-label="Game log">
  <header>
    <h2>Game log <span>· {name}</span></h2>
    <div class="tools">
      <span class="count">{lines.length} lines</span>
      <button class="btn ghost" onclick={copy} disabled={lines.length === 0}>{copied ? "Copied" : "Copy"}</button>
      <button class="btn ghost square" aria-label="Close log" onclick={() => (games.logOpenFor = null)}>
        <svg width="14" height="14" viewBox="0 0 14 14" stroke="currentColor" stroke-width="1.8" stroke-linecap="round">
          <path d="M3 3l8 8M11 3l-8 8" />
        </svg>
      </button>
    </div>
  </header>
  <div class="lines" bind:this={scroller} {onscroll}>
    {#if lines.length === 0}
      <p class="empty">Nothing yet. Press Play and the game's output shows up here.</p>
    {/if}
    {#each lines as line, i (i)}
      <div class="line {line.level.toLowerCase()}">
        <span class="time">{logTime(line.timestamp)}</span>
        <span class="msg">{line.message}</span>
      </div>
    {/each}
  </div>
</section>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    border-top: 1px solid var(--line);
    background: var(--panel);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 6px 10px 6px 16px;
    border-bottom: 1px solid var(--line);
  }
  h2 {
    font-size: 13px;
  }
  h2 span {
    color: var(--muted);
    font-weight: 400;
  }
  .tools {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .count {
    color: var(--faint);
    font-size: 12px;
    margin-right: 6px;
  }
  .tools .btn {
    height: 28px;
  }
  .tools .square {
    width: 28px;
  }
  .lines {
    flex: 1;
    overflow: auto;
    padding: 8px 0;
    font-family: var(--mono);
    font-size: 12px;
    line-height: 1.5;
    user-select: text;
  }
  .empty {
    padding: 8px 16px;
    color: var(--faint);
    font-family: inherit;
  }
  .line {
    display: flex;
    gap: 12px;
    padding: 0 16px;
  }
  .line:hover {
    background: rgb(255 255 255 / 0.03);
  }
  .time {
    flex: none;
    width: 62px;
    color: var(--faint);
  }
  .msg {
    white-space: pre-wrap;
    word-break: break-word;
    color: #ddd2c5;
  }
  .warn .msg {
    color: var(--warn);
  }
  .error .msg,
  .fatal .msg {
    color: #ff8f86;
  }
  .debug .msg {
    color: var(--faint);
  }
</style>
