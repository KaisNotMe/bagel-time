<script lang="ts">
  import ModrinthBrowser from "$lib/components/ModrinthBrowser.svelte";
  import { packs } from "$lib/packs.svelte";
</script>

<div class="scroll">
  <header>
    <div>
      <h1>Modpacks</h1>
      <p class="sub">Install a pack from Modrinth as a new instance, or drop a .mrpack file anywhere.</p>
    </div>
    <button class="btn" disabled={packs.busy} onclick={() => packs.pickFile()}>
      <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
        <path d="M8 2v8M4.5 6.5 8 10l3.5-3.5M3 13.5h10" />
      </svg>
      Import .mrpack
    </button>
  </header>

  <ModrinthBrowser projectType="modpack" placeholder="Search modpacks">
    {#snippet action(hit)}
      <button
        class="btn primary"
        disabled={packs.busy}
        onclick={() => packs.installFromModrinth(hit.projectId, hit.title)}
      >
        {packs.installing === hit.projectId ? "Installing…" : "Install"}
      </button>
    {/snippet}
  </ModrinthBrowser>
</div>

<style>
  .scroll {
    height: 100%;
    overflow: auto;
    padding: 28px 32px 40px;
  }
  header {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 22px;
  }
  h1 {
    font-size: 26px;
    letter-spacing: -0.3px;
  }
  .sub {
    color: var(--muted);
  }
</style>
