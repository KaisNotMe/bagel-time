<script lang="ts">
  import type { Snippet } from "svelte";
  import { games } from "$lib/games.svelte";
  import { servers } from "$lib/servers.svelte";
  import Icon from "./Icon.svelte";
  import Motd from "./Motd.svelte";

  type Props = {
    instanceId: string;
    name: string;
    address: string;
    /** Icon the game saved, shown until a ping brings a fresh one. */
    icon?: string | null;
    /** Small line under the name, e.g. the instance and when it was played. */
    note?: string | null;
    /** Extra buttons after Join. */
    actions?: Snippet;
  };
  let { instanceId, name, address, icon = null, note = null, actions }: Props = $props();

  $effect(() => {
    servers.ping(address);
  });

  let ping = $derived(servers.get(address));
  let status = $derived(ping?.state === "online" ? ping.status : null);
  let shownIcon = $derived(status?.icon ?? icon);
  let game = $derived(games.status[instanceId]);
  let bars = $derived(!status ? 0 : status.pingMs < 80 ? 4 : status.pingMs < 150 ? 3 : status.pingMs < 300 ? 2 : 1);
</script>

<div class="server card">
  {#if shownIcon}
    <img src={shownIcon} alt="" class="icon" />
  {:else}
    <div class="icon placeholder"><Icon name="server" size={24} stroke={1.5} /></div>
  {/if}
  <div class="info">
    <h3 title={address}>{name}</h3>
    {#if !ping || ping.state === "loading"}
      <span class="faint small">Checking…</span>
    {:else if status}
      <Motd text={status.motd || address} />
    {:else}
      <span class="offline small" title={ping.state === "offline" ? ping.error : undefined}>
        <span class="dot"></span> Can't reach this server
      </span>
    {/if}
    {#if note}<small class="faint note">{note}</small>{/if}
  </div>
  <div class="stats">
    {#if status}
      <span class="players" title={status.sample.length ? status.sample.join(", ") : `${status.online} playing`}>
        <Icon name="users" size={14} />
        {status.online.toLocaleString()}<span class="faint">/{status.max.toLocaleString()}</span>
      </span>
      <span class="ping" class:good={bars >= 3} class:ok={bars === 2} class:bad={bars === 1} title="{status.pingMs} ms">
        <Icon name="signal" size={14} />
        {status.pingMs} ms
      </span>
    {/if}
  </div>
  <div class="buttons">
    <button
      class="btn primary sm"
      disabled={!!game}
      title={game ? "This instance is already running" : `Start the game and join ${address}`}
      onclick={() => games.launch(instanceId, address)}
    >
      <Icon name="play" size={13} fill /> Join
    </button>
    {@render actions?.()}
  </div>
</div>

<style>
  .server {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 10px 12px;
  }
  .icon {
    flex: none;
    width: 52px;
    height: 52px;
    border-radius: 8px;
    image-rendering: pixelated;
  }
  .placeholder {
    display: grid;
    place-items: center;
    background: var(--raised-3);
    color: var(--faint);
  }
  .info {
    flex: 1;
    min-width: 0;
    display: grid;
    gap: 3px;
  }
  h3 {
    font-size: 15px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .note {
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .small {
    font-size: 12.5px;
  }
  .offline {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--faint);
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--danger);
  }
  .stats {
    display: grid;
    justify-items: end;
    gap: 2px;
    font-size: 12.5px;
    color: var(--muted);
    white-space: nowrap;
  }
  .players,
  .ping {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .ping.good {
    color: var(--ok);
  }
  .ping.ok {
    color: var(--warn);
  }
  .ping.bad {
    color: var(--danger);
  }
  .buttons {
    display: flex;
    gap: 4px;
  }
</style>
