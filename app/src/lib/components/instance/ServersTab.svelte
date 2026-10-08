<script lang="ts">
  import { api, errorMessage, type Instance, type Server } from "$lib/api";
  import { relativeTime } from "$lib/format";
  import { games } from "$lib/games.svelte";
  import { servers } from "$lib/servers.svelte";
  import Icon from "../Icon.svelte";
  import ServerCard from "../ServerCard.svelte";

  let { instance }: { instance: Instance } = $props();

  let list = $state<Server[]>([]);
  let loaded = $state(false);
  let error = $state("");
  let adding = $state(false);
  let name = $state("");
  let address = $state("");
  let saving = $state(false);

  let running = $derived(!!games.status[instance.id]);
  /** Joined recently but not in the list (direct connect, or removed since). */
  let recentOnly = $derived(
    servers.recent.filter(
      (r) => r.instanceId === instance.id && !list.some((s) => sameAddress(s.address, r.address)),
    ),
  );

  function sameAddress(a: string, b: string) {
    const norm = (s: string) => s.trim().toLowerCase().replace(/:25565$/, "");
    return norm(a) === norm(b);
  }

  async function load() {
    try {
      list = await api.listServers(instance.id);
      error = "";
    } catch (e) {
      error = errorMessage(e);
    } finally {
      loaded = true;
    }
  }

  $effect(() => {
    instance.id;
    loaded = false;
    load();
    servers.loadRecent();
  });

  // The game edits the list too; pick up its changes once it closes.
  let wasRunning = false;
  $effect(() => {
    if (wasRunning && !running) load();
    wasRunning = running;
  });

  async function save(e: SubmitEvent) {
    e.preventDefault();
    saving = true;
    error = "";
    try {
      await api.addServer(instance.id, name, address);
      name = "";
      address = "";
      adding = false;
      await load();
    } catch (err) {
      error = errorMessage(err);
    } finally {
      saving = false;
    }
  }

  async function keep(serverName: string, serverAddress: string) {
    try {
      await api.addServer(instance.id, serverName, serverAddress);
      await load();
    } catch (err) {
      error = errorMessage(err);
    }
  }

  async function remove(serverAddress: string) {
    try {
      await api.removeServer(instance.id, serverAddress);
      await load();
    } catch (err) {
      error = errorMessage(err);
    }
  }

  function refresh() {
    for (const s of [...list, ...recentOnly]) servers.ping(s.address, true);
  }

  function playedNote(addr: string) {
    const r = servers.recent.find((r) => r.instanceId === instance.id && sameAddress(r.address, addr));
    return r ? relativeTime(r.lastPlayed) : null;
  }
</script>

<div class="tab">
  <div class="toolbar">
    <p class="muted hint">
      {running ? "Close the game to change this list." : "Same list as Multiplayer in the game. Press Join to start and connect in one go."}
    </p>
    <button class="btn" onclick={refresh} disabled={list.length + recentOnly.length === 0}>
      <Icon name="refresh" size={16} /> Refresh
    </button>
    <button class="btn primary" onclick={() => (adding = !adding)} disabled={running}>
      <Icon name="plus" size={16} /> Add server
    </button>
  </div>

  {#if adding && !running}
    <form class="add card" onsubmit={save}>
      <label class="field">
        <span>Name</span>
        <input class="input" bind:value={name} placeholder="Friends SMP" maxlength="60" />
      </label>
      <label class="field grow">
        <span>Address</span>
        <input class="input" bind:value={address} placeholder="play.example.net" required spellcheck="false" />
      </label>
      <div class="add-buttons">
        <button class="btn ghost" type="button" onclick={() => (adding = false)}>Cancel</button>
        <button class="btn primary" type="submit" disabled={saving || !address.trim()}>
          {saving ? "Adding…" : "Add"}
        </button>
      </div>
    </form>
  {/if}

  {#if error}<p class="alert">{error}</p>{/if}

  {#if loaded && list.length === 0 && recentOnly.length === 0}
    <div class="empty-state card">
      <Icon name="server" size={36} stroke={1.4} />
      <h3>No servers yet</h3>
      <p>Add the server your friends play on, then press Join to hop straight in.</p>
    </div>
  {:else}
    <div class="list">
      {#each list as s (s.address)}
        <ServerCard instanceId={instance.id} name={s.name} address={s.address} icon={s.icon} note={playedNote(s.address)}>
          {#snippet actions()}
            <button
              class="btn ghost square sm"
              title={running ? "Close the game first" : "Remove from list"}
              aria-label="Remove {s.name}"
              disabled={running}
              onclick={() => remove(s.address)}
            >
              <Icon name="trash" size={15} />
            </button>
          {/snippet}
        </ServerCard>
      {/each}
    </div>

    {#if recentOnly.length}
      <h3 class="section-title sub">Recently joined</h3>
      <div class="list">
        {#each recentOnly as r (r.address)}
          <ServerCard instanceId={instance.id} name={r.name} address={r.address} note={relativeTime(r.lastPlayed)}>
            {#snippet actions()}
              <button
                class="btn ghost square sm"
                title={running ? "Close the game first" : "Save to the server list"}
                aria-label="Save {r.name}"
                disabled={running}
                onclick={() => keep(r.name, r.address)}
              >
                <Icon name="plus" size={15} />
              </button>
            {/snippet}
          </ServerCard>
        {/each}
      </div>
    {/if}
  {/if}
</div>

<style>
  .tab {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .hint {
    flex: 1;
    font-size: 13px;
  }
  .add {
    display: flex;
    align-items: flex-end;
    gap: 10px;
    padding: 14px;
  }
  .add .field:first-child {
    width: 200px;
  }
  .grow {
    flex: 1;
  }
  .add-buttons {
    display: flex;
    gap: 6px;
  }
  .list {
    display: grid;
    gap: 8px;
  }
  .sub {
    font-size: 15px;
    margin-top: 6px;
  }
</style>
