<script lang="ts">
  import { tick } from "svelte";
  import { api, errorMessage, type Instance, type LogFile } from "$lib/api";
  import { logTime } from "$lib/format";
  import { games } from "$lib/games.svelte";
  import Icon from "../Icon.svelte";

  let { instance }: { instance: Instance } = $props();

  const LIVE = "live";
  const MAX_LINES = 5000;

  type Line = { level: string; time: string; text: string };

  let files = $state<LogFile[]>([]);
  let source = $state(LIVE);
  let fileText = $state("");
  let loading = $state(false);
  let error = $state("");
  let search = $state("");
  let scroller = $state<HTMLDivElement>();
  let followTail = $state(true);
  let copied = $state(false);

  async function loadFiles() {
    try {
      files = await api.listLogFiles(instance.id);
    } catch (e) {
      error = errorMessage(e);
    }
  }

  $effect(() => {
    void instance.id;
    source = LIVE;
    loadFiles();
  });

  // Refresh the file list when a game exits (new latest.log / crash report).
  $effect(() => games.onChange(loadFiles));

  $effect(() => {
    if (source === LIVE) return;
    const name = source;
    loading = true;
    error = "";
    api
      .readLogFile(instance.id, name)
      .then((t) => {
        if (source === name) fileText = t;
      })
      .catch((e) => (error = errorMessage(e)))
      .finally(() => (loading = false));
  });

  function levelOf(text: string): string {
    const m = /\/(FATAL|ERROR|WARN|DEBUG)\]|\[(FATAL|ERROR|WARN|DEBUG)\]/.exec(text);
    return (m?.[1] ?? m?.[2] ?? (/^\s*(at |Caused by:|java\.|\w+(\.\w+)+Exception)/.test(text) ? "ERROR" : "INFO")).toLowerCase();
  }

  let liveLines = $derived(games.logs[instance.id] ?? []);
  let allLines = $derived.by<Line[]>(() => {
    if (source === LIVE) {
      return liveLines.flatMap((l) =>
        l.message.split("\n").map((text, i) => ({
          level: l.level.toLowerCase(),
          time: i === 0 ? logTime(l.timestamp) : "",
          text,
        })),
      );
    }
    return fileText.split(/\r?\n/).map((text) => ({ level: levelOf(text), time: "", text }));
  });
  let filtered = $derived(
    search.trim() ? allLines.filter((l) => l.text.toLowerCase().includes(search.trim().toLowerCase())) : allLines,
  );
  let shown = $derived(filtered.slice(-MAX_LINES));

  $effect(() => {
    void shown.length;
    void source;
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
    await navigator.clipboard.writeText(filtered.map((l) => (l.time ? `[${l.time}] ${l.text}` : l.text)).join("\n"));
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }

  function label(f: LogFile) {
    const name = f.name.split("/").pop() ?? f.name;
    return f.crashReport ? `Crash report · ${name}` : name;
  }
</script>

<div class="tab">
  <div class="toolbar">
    <select class="input source" bind:value={source} aria-label="Log source">
      <option value={LIVE}>Live output{games.status[instance.id] ? " (running)" : ""}</option>
      {#each files as f (f.name)}
        <option value={f.name}>{label(f)}</option>
      {/each}
    </select>
    <label class="search-box">
      <Icon name="search" size={16} />
      <input class="input" type="search" placeholder="Search log" bind:value={search} />
    </label>
    <button class="btn" onclick={copy} disabled={filtered.length === 0}>
      <Icon name={copied ? "check" : "copy"} size={16} />
      {copied ? "Copied" : "Copy"}
    </button>
    <button class="btn ghost square" title="Open logs folder" aria-label="Open logs folder" onclick={() => api.openInstanceFolder(instance.id, "logs")}>
      <Icon name="folder" size={17} />
    </button>
  </div>

  {#if error}<p class="alert">{error}</p>{/if}

  <div class="log card" bind:this={scroller} {onscroll}>
    {#if loading}
      <p class="empty">Loading…</p>
    {:else if allLines.length === 0 || (allLines.length === 1 && !allLines[0].text)}
      <p class="empty">
        {source === LIVE ? "Nothing yet. Press Play and the game's output shows up here." : "This log is empty."}
      </p>
    {:else}
      {#if filtered.length > MAX_LINES}
        <p class="note">Showing the last {MAX_LINES.toLocaleString()} of {filtered.length.toLocaleString()} lines.</p>
      {/if}
      {#each shown as line, i (i)}
        <div class="line {line.level}">
          {#if source === LIVE}<span class="time">{line.time}</span>{/if}
          <span class="msg">{line.text}</span>
        </div>
      {/each}
    {/if}
  </div>
</div>

<style>
  .tab {
    display: flex;
    flex-direction: column;
    gap: 12px;
    height: 100%;
    min-height: 0;
  }
  .toolbar {
    display: flex;
    gap: 8px;
  }
  .source {
    width: 260px;
  }
  .log {
    flex: 1;
    min-height: 360px;
    overflow: auto;
    padding: 10px 0;
    background: var(--rail);
    font-family: var(--mono);
    font-size: 12.5px;
    line-height: 1.55;
  }
  .line {
    display: flex;
    gap: 12px;
    padding: 0 14px;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .line:hover {
    background: var(--raised);
  }
  .time {
    flex: none;
    width: 64px;
    color: var(--faint);
  }
  .msg {
    min-width: 0;
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
  .empty,
  .note {
    padding: 16px;
    color: var(--faint);
    font-family: inherit;
  }
  .note {
    padding: 0 14px 8px;
  }
</style>
