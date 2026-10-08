<script lang="ts">
  import { LOADER_NAMES, type HostedServer } from "$lib/api";
  import { hosting } from "$lib/hosting.svelte";
  import CopyAddress from "./CopyAddress.svelte";
  import Icon from "./Icon.svelte";

  let { server }: { server: HostedServer } = $props();

  let phase = $derived(server.status.phase);
  let label = $derived(
    phase === "running"
      ? server.status.players.length
        ? `Online · ${server.status.players.length} playing`
        : "Online"
      : phase === "starting"
        ? server.status.stage || "Starting…"
        : phase === "stopping"
          ? "Stopping…"
          : "Offline",
  );
  let address = $derived(server.tunnel ? (server.status.publicAddress ?? server.publicAddress) : null);
</script>

<div class="card hosted">
  <a class="main" href="/hosted?id={encodeURIComponent(server.id)}">
    <span class="icon" class:on={phase === "running"}><Icon name="server" size={24} stroke={1.6} /></span>
    <span class="text">
      <b>{server.name}</b>
      <small>{LOADER_NAMES[server.loader]} {server.gameVersion}</small>
      <small class="state {phase}"><span class="dot"></span>{label}</small>
    </span>
  </a>
  <div class="side">
    {#if address}
      <CopyAddress {address} small />
    {/if}
    {#if phase === "stopped"}
      <button class="btn primary sm" onclick={() => hosting.start(server.id)}>
        <Icon name="play" size={13} fill /> Start
      </button>
    {:else}
      <button class="btn sm" disabled={phase === "stopping"} onclick={() => hosting.stop(server.id)}>
        <Icon name="stop" size={12} fill /> Stop
      </button>
    {/if}
  </div>
</div>

<style>
  .hosted {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px;
  }
  .main {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
    color: inherit;
    text-decoration: none;
  }
  .icon {
    flex: none;
    display: grid;
    place-items: center;
    width: 52px;
    height: 52px;
    border-radius: 12px;
    background: var(--raised-3);
    color: var(--faint);
  }
  .icon.on {
    background: var(--ok-soft);
    color: var(--ok);
  }
  .text {
    display: grid;
    min-width: 0;
    line-height: 1.35;
  }
  .text b,
  .text small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .text small {
    color: var(--muted);
    font-size: 12.5px;
  }
  .state {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .dot {
    flex: none;
    width: 7px;
    height: 7px;
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
  .side {
    display: grid;
    justify-items: end;
    gap: 6px;
  }
</style>
