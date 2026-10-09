<script lang="ts">
  // One-time link between Bagel Time and playit.gg: open the claim page,
  // wait for the player to approve it, done.
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { onDestroy } from "svelte";
  import { api, errorMessage, type PlayitClaim } from "$lib/api";
  import { hosting } from "$lib/hosting.svelte";
  import Icon from "./Icon.svelte";

  let { compact = false }: { compact?: boolean } = $props();

  let claim = $state<PlayitClaim | null>(null);
  let step = $state<"idle" | "waiting" | "rejected">("idle");
  let error = $state("");
  let timer: ReturnType<typeof setTimeout> | null = null;
  let started = 0;

  onDestroy(() => {
    if (timer) clearTimeout(timer);
  });

  async function connect() {
    error = "";
    try {
      claim = await api.playitStartClaim();
      step = "waiting";
      started = Date.now();
      await openUrl(claim.url);
      poll();
    } catch (e) {
      error = errorMessage(e);
      step = "idle";
    }
  }

  async function poll() {
    if (!claim) return;
    try {
      const result = await api.playitPollClaim(claim.code);
      if (result === "connected") {
        hosting.playitConnected = true;
        step = "idle";
        claim = null;
        return;
      }
      if (result === "rejected") {
        step = "rejected";
        return;
      }
    } catch (e) {
      error = errorMessage(e);
    }
    // Claim codes don't last forever; stop after 15 minutes.
    if (Date.now() - started > 15 * 60_000) {
      step = "idle";
      error = "That took a while. Press Connect to try again.";
      return;
    }
    timer = setTimeout(poll, 2000);
  }
</script>

{#if hosting.playitConnected}
  <p class="done"><Icon name="check" size={15} /> playit.gg is connected</p>
{:else}
  <div class="box" class:compact>
    <div class="text">
      <b>Connect playit.gg (one time)</b>
      {#if step === "waiting"}
        <small>
          Approve it in the browser page that opened (a guest account is fine).
          <button class="link" onclick={() => claim && openUrl(claim.url)}>Open it again</button>
        </small>
      {:else if step === "rejected"}
        <small class="bad">It wasn't approved. Press Connect to try again.</small>
      {:else}
        <small>Free. Gives your server an address friends can join.</small>
      {/if}
      {#if error}<small class="bad">{error}</small>{/if}
    </div>
    <button class="btn primary sm" type="button" onclick={connect} disabled={step === "waiting"}>
      {step === "waiting" ? "Waiting…" : "Connect"}
    </button>
  </div>
{/if}

<style>
  .box {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px;
    border-radius: 10px;
    background: var(--raised-2);
  }
  .text {
    flex: 1;
    display: grid;
    gap: 3px;
    min-width: 0;
  }
  .text small {
    color: var(--muted);
    font-size: 12.5px;
  }
  .text .bad {
    color: #ff8f86;
  }
  .done {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--ok);
    font-size: 13px;
  }
  .link {
    border: 0;
    background: none;
    padding: 0;
    color: var(--accent, var(--primary));
    text-decoration: underline;
    cursor: pointer;
    font: inherit;
  }
</style>
