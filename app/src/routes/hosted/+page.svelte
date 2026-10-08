<script lang="ts">
  import { goto } from "$app/navigation";
  import { page } from "$app/state";
  import { tick } from "svelte";
  import { api, errorMessage, LOADER_NAMES } from "$lib/api";
  import CopyAddress from "$lib/components/CopyAddress.svelte";
  import Icon, { type IconName } from "$lib/components/Icon.svelte";
  import PlayitConnect from "$lib/components/PlayitConnect.svelte";
  import { games } from "$lib/games.svelte";
  import { hosting } from "$lib/hosting.svelte";
  import { instances } from "$lib/instances.svelte";
  import { ui } from "$lib/ui.svelte";

  type Tab = "console" | "players" | "settings";
  const tabs: { id: Tab; label: string; icon: IconName }[] = [
    { id: "console", label: "Console", icon: "terminal" },
    { id: "players", label: "Players", icon: "users" },
    { id: "settings", label: "Settings", icon: "settings" },
  ];

  let id = $derived(page.url.searchParams.get("id") ?? "");
  let tab = $state<Tab>("console");
  let server = $derived(hosting.get(id));
  let phase = $derived(server?.status.phase ?? "stopped");
  let lines = $derived(hosting.consoles[id] ?? []);
  let address = $derived(server?.tunnel ? (server.status.publicAddress ?? server.publicAddress) : null);
  let instance = $derived(server?.fromInstance ? instances.get(server.fromInstance) : undefined);
  let localAddress = $derived(server ? (server.port === 25565 ? "localhost" : `localhost:${server.port}`) : "");

  let command = $state("");
  let consoleEl = $state<HTMLDivElement | null>(null);
  let stick = $state(true);

  // Settings form.
  let form = $state({ name: "", memoryGb: 4, onlineMode: false, whitelist: false, tunnel: true });
  let saving = $state(false);
  let saved = $state(false);
  let settingsError = $state("");
  let confirmDelete = $state(false);

  $effect(() => {
    if (!id) return;
    hosting.loadConsole(id);
    hosting.checkPlayit();
    if (!hosting.loaded) hosting.refresh();
  });

  $effect(() => {
    if (server && tab !== "settings") {
      form = {
        name: server.name,
        memoryGb: Math.round(server.memoryMb / 1024),
        onlineMode: server.onlineMode,
        whitelist: server.whitelist,
        tunnel: server.tunnel,
      };
    }
  });

  $effect(() => {
    ui.setCrumbs({ label: "Home", href: "/" }, { label: server?.name ?? "Server" });
  });

  // Follow new console lines unless the player scrolled up.
  $effect(() => {
    lines.length;
    if (stick && consoleEl) tick().then(() => consoleEl && (consoleEl.scrollTop = consoleEl.scrollHeight));
  });

  function onScroll() {
    if (!consoleEl) return;
    stick = consoleEl.scrollHeight - consoleEl.scrollTop - consoleEl.clientHeight < 40;
  }

  async function send(e: SubmitEvent) {
    e.preventDefault();
    const text = command.trim();
    if (!text) return;
    try {
      await api.sendServerCommand(id, text);
      command = "";
    } catch (err) {
      hosting.errors[id] = errorMessage(err);
    }
  }

  async function saveSettings(e: SubmitEvent) {
    e.preventDefault();
    saving = true;
    settingsError = "";
    try {
      const updated = await api.updateHostedServer(id, {
        name: form.name,
        memoryMb: Math.round(form.memoryGb * 1024),
        onlineMode: form.onlineMode,
        whitelist: form.whitelist,
        tunnel: form.tunnel,
      });
      const i = hosting.list.findIndex((s) => s.id === id);
      if (i !== -1) hosting.list[i] = updated;
      saved = true;
      setTimeout(() => (saved = false), 1500);
    } catch (err) {
      settingsError = errorMessage(err);
    } finally {
      saving = false;
    }
  }

  async function remove() {
    try {
      await api.deleteHostedServer(id);
      await hosting.refresh();
      goto("/");
    } catch (err) {
      settingsError = errorMessage(err);
    }
  }

  function levelClass(level: string) {
    return level === "ERROR" || level === "FATAL" ? "err" : level === "WARN" ? "warn" : "";
  }
</script>

<div class="page">
  <div class="page-inner">
    {#if !server}
      {#if hosting.loaded}
        <p class="alert">This server doesn't exist anymore.</p>
      {:else}
        <div class="hero card skeleton"></div>
      {/if}
    {:else}
      <header class="hero card">
        <span class="icon" class:on={phase === "running"}><Icon name="server" size={40} stroke={1.4} /></span>
        <div class="info">
          <h1>{server.name}</h1>
          <p class="meta">
            <span class="chip"><Icon name={server.loader === "vanilla" ? "box" : "puzzle"} size={14} />{LOADER_NAMES[server.loader]} {server.gameVersion}</span>
            <span class="chip state {phase}">
              <span class="dot"></span>
              {phase === "running" ? "Online" : phase === "starting" ? server.status.stage || "Starting…" : phase === "stopping" ? server.status.stage || "Stopping…" : "Offline"}
            </span>
            {#if phase === "running"}<span class="chip"><Icon name="users" size={14} />{server.status.players.length} playing</span>{/if}
          </p>
          {#if hosting.errors[id]}
            <div class="alert row">
              <span>{hosting.errors[id]}</span>
              <button class="btn ghost sm" onclick={() => delete hosting.errors[id]}>Dismiss</button>
            </div>
          {/if}
        </div>
        <div class="actions">
          {#if phase === "stopped"}
            <button class="btn lg primary" onclick={() => hosting.start(id)}>
              <Icon name="play" size={17} fill /> Start
            </button>
          {:else}
            <button class="btn lg danger" disabled={phase === "stopping"} onclick={() => hosting.stop(id)}>
              <Icon name="stop" size={16} fill /> {phase === "stopping" ? "Stopping…" : "Stop"}
            </button>
          {/if}
          <button class="btn lg square" title="Open server folder" aria-label="Open server folder" onclick={() => api.openHostedFolder(id)}>
            <Icon name="folder" size={19} />
          </button>
        </div>
      </header>

      <section class="share card">
        <div class="share-main">
          <h2>Invite your friends</h2>
          {#if server.tunnel && address}
            <p class="muted">Send them this address. They add it under Multiplayer (or in Bagel Time's Servers tab) and join.</p>
            <CopyAddress {address} />
          {:else if server.tunnel && hosting.playitConnected === false}
            <p class="muted">Connect playit.gg once, then start the server to get an address your friends can use.</p>
            <PlayitConnect />
          {:else if server.tunnel}
            <p class="muted">
              {phase === "stopped" ? "Start the server to get its public address." : "Getting the public address…"}
            </p>
          {:else}
            <p class="muted">Only people on your network can join. Turn on "Let friends join from anywhere" in Settings to share it.</p>
          {/if}
          {#if server.status.tunnelError && !(server.tunnel && hosting.playitConnected === false)}
            <p class="warn small">{server.status.tunnelError}</p>
          {/if}
          {#if !server.onlineMode && server.tunnel}
            <p class="faint small">
              Anyone with the address can join under any name. Turn on the whitelist in Settings to limit it to your friends.
            </p>
          {/if}
        </div>
        <div class="share-side">
          <span class="faint small">On this computer</span>
          <CopyAddress address={localAddress} small />
          {#if instance}
            <button
              class="btn primary sm"
              disabled={phase !== "running" || !!games.status[instance.id]}
              title={phase !== "running" ? "Start the server first" : `Play ${instance.name} and join`}
              onclick={() => games.launch(instance!.id, localAddress)}
            >
              <Icon name="play" size={13} fill /> Play on it
            </button>
          {/if}
        </div>
      </section>

      {#if hosting.skipped[id]?.length}
        <div class="alert row note">
          <span>
            Left out because they only work in the game client: {hosting.skipped[id].join(", ")}.
          </span>
          <button class="btn ghost sm" onclick={() => delete hosting.skipped[id]}>OK</button>
        </div>
      {/if}

      <nav class="pills" aria-label="Server sections">
        {#each tabs as t (t.id)}
          <button class="pill" class:active={tab === t.id} onclick={() => (tab = t.id)}>
            <Icon name={t.icon} size={15} />
            {t.label}
          </button>
        {/each}
      </nav>

      {#if tab === "console"}
        <div class="console card">
          <div class="lines" bind:this={consoleEl} onscroll={onScroll}>
            {#each lines as line, i (i)}
              <div class={levelClass(line.level)}>{line.message}</div>
            {:else}
              <p class="faint">{phase === "stopped" ? "Start the server to see its console." : "Waiting for the server…"}</p>
            {/each}
          </div>
          <form class="cmd" onsubmit={send}>
            <span class="prompt">&gt;</span>
            <input
              class="input mono"
              bind:value={command}
              placeholder={phase === "running" ? "Type a command, e.g. op YourName or whitelist add Friend" : "The server isn't running"}
              disabled={phase !== "running"}
              spellcheck="false"
              autocomplete="off"
            />
            <button class="btn primary sm" disabled={phase !== "running" || !command.trim()}>Send</button>
          </form>
        </div>
      {:else if tab === "players"}
        <div class="card players">
          {#each server.status.players as name (name)}
            <div class="player">
              <span class="avatar">{name[0].toUpperCase()}</span>
              <b>{name}</b>
              <span class="grow"></span>
              <button class="btn ghost sm" onclick={() => api.sendServerCommand(id, `op ${name}`)}>Make operator</button>
              <button class="btn ghost sm" onclick={() => api.sendServerCommand(id, `kick ${name}`)}>Kick</button>
            </div>
          {:else}
            <p class="faint empty">{phase === "running" ? "Nobody is online right now." : "The server is offline."}</p>
          {/each}
        </div>
      {:else}
        <form class="card settings" onsubmit={saveSettings}>
          {#if phase !== "stopped"}<p class="warn small">Stop the server to change its settings.</p>{/if}
          <fieldset disabled={phase !== "stopped"}>
            <label class="field">
              <span>Name</span>
              <input class="input" bind:value={form.name} maxlength="60" />
            </label>
            <label class="field">
              <span>Memory: {form.memoryGb} GB</span>
              <input type="range" min="1" max="16" step="1" bind:value={form.memoryGb} />
            </label>
            <label class="check">
              <input type="checkbox" bind:checked={form.tunnel} />
              <span>Let friends join from anywhere <small class="muted">(playit.gg tunnel)</small></span>
            </label>
            <label class="check">
              <input type="checkbox" bind:checked={form.onlineMode} />
              <span>Only players signed in with a Microsoft account</span>
            </label>
            <label class="check">
              <input type="checkbox" bind:checked={form.whitelist} />
              <span>
                Whitelist <small class="muted">(only listed players can join; add them in the console with
                  <code>whitelist add Name</code>)</small>
              </span>
            </label>
          </fieldset>
          {#if settingsError}<p class="alert">{settingsError}</p>{/if}
          <div class="settings-actions">
            {#if confirmDelete}
              <span class="warn small">Delete the server and its world for good?</span>
              <button class="btn danger sm" type="button" onclick={remove}>Delete</button>
              <button class="btn ghost sm" type="button" onclick={() => (confirmDelete = false)}>Cancel</button>
            {:else}
              <button class="btn ghost sm danger-text" type="button" disabled={phase !== "stopped"} onclick={() => (confirmDelete = true)}>
                <Icon name="trash" size={14} /> Delete server
              </button>
            {/if}
            <span class="grow"></span>
            <button class="btn primary" disabled={saving || phase !== "stopped"}>
              {saved ? "Saved" : saving ? "Saving…" : "Save"}
            </button>
          </div>
        </form>
      {/if}
    {/if}
  </div>
</div>

<style>
  .hero {
    display: flex;
    align-items: center;
    gap: 20px;
    padding: 20px;
    margin-bottom: 14px;
  }
  .skeleton {
    height: 136px;
  }
  .icon {
    flex: none;
    display: grid;
    place-items: center;
    width: 96px;
    height: 96px;
    border-radius: 20px;
    background: var(--raised-3);
    color: var(--faint);
  }
  .icon.on {
    background: var(--ok-soft);
    color: var(--ok);
  }
  .info {
    flex: 1;
    min-width: 0;
    display: grid;
    gap: 8px;
  }
  h1 {
    font-size: 26px;
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    border-radius: 999px;
    background: var(--raised-2);
    color: var(--muted);
    font-size: 13px;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--faint);
  }
  .state.running .dot {
    background: var(--ok);
  }
  .state.starting .dot,
  .state.stopping .dot {
    background: var(--warn);
  }
  .actions {
    display: flex;
    gap: 8px;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }
  .share {
    display: flex;
    gap: 20px;
    padding: 18px 20px;
    margin-bottom: 14px;
  }
  .share-main {
    flex: 1;
    min-width: 0;
    display: grid;
    gap: 8px;
    justify-items: start;
  }
  .share h2 {
    font-size: 17px;
  }
  .share-side {
    display: grid;
    gap: 6px;
    justify-items: end;
    align-content: start;
  }
  .small {
    font-size: 12.5px;
  }
  .warn {
    color: var(--warn);
  }
  .note {
    margin-bottom: 14px;
  }
  .console {
    display: flex;
    flex-direction: column;
    height: 460px;
    padding: 0;
    overflow: hidden;
  }
  .lines {
    flex: 1;
    overflow: auto;
    padding: 12px 14px;
    font-family: var(--mono);
    font-size: 12px;
    line-height: 1.5;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .lines .err {
    color: #ff8f86;
  }
  .lines .warn {
    color: var(--warn);
  }
  .cmd {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    border-top: 1px solid var(--line);
  }
  .prompt {
    font-family: var(--mono);
    color: var(--faint);
  }
  .cmd .input {
    flex: 1;
  }
  .mono {
    font-family: var(--mono);
  }
  .players {
    display: grid;
    gap: 2px;
    padding: 8px;
  }
  .player {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 8px;
    border-radius: 8px;
  }
  .avatar {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: 8px;
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 800;
  }
  .empty {
    padding: 16px;
    text-align: center;
  }
  .grow {
    flex: 1;
  }
  .settings {
    display: grid;
    gap: 14px;
    padding: 18px 20px;
  }
  fieldset {
    display: grid;
    gap: 14px;
    border: 0;
    padding: 0;
    margin: 0;
  }
  .check {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    cursor: pointer;
  }
  .check input {
    margin-top: 3px;
  }
  .settings-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .danger-text {
    color: var(--danger);
  }
  code {
    font-family: var(--mono);
    font-size: 12px;
  }
</style>
