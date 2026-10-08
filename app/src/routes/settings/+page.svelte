<script lang="ts">
  import { onMount } from "svelte";
  import { api, errorMessage, type Settings } from "$lib/api";

  let settings = $state<Settings | null>(null);
  let saving = $state(false);
  let message = $state<{ ok: boolean; text: string } | null>(null);

  onMount(async () => {
    try {
      settings = await api.getSettings();
    } catch (e) {
      message = { ok: false, text: errorMessage(e) };
    }
  });

  async function save(e: SubmitEvent) {
    e.preventDefault();
    if (!settings) return;
    saving = true;
    message = null;
    try {
      settings = await api.saveSettings($state.snapshot(settings));
      message = { ok: true, text: "Saved." };
    } catch (err) {
      message = { ok: false, text: errorMessage(err) };
    } finally {
      saving = false;
    }
  }
</script>

<div class="scroll">
  <h1>Settings</h1>

  {#if settings}
    <form onsubmit={save}>
      <section>
        <h2>Account</h2>
        <p class="note">
          Microsoft login is coming next. Until then you play in offline mode: singleplayer and
          offline-mode servers work, online servers and skins don't.
        </p>
        <label class="field">
          <span>Offline username</span>
          <input class="input" bind:value={settings.offlineUsername} maxlength="16" spellcheck="false" />
          <small>3–16 characters: letters, numbers and _</small>
        </label>
      </section>

      <section>
        <h2>Java & memory</h2>
        <label class="field">
          <span>Default maximum memory <b>{(settings.memoryMb / 1024).toFixed(1)} GB</b></span>
          <input type="range" min="1024" max="16384" step="512" bind:value={settings.memoryMb} />
          <small>4 GB suits vanilla. Big modpacks usually want 6–8 GB. Java is downloaded automatically.</small>
        </label>
      </section>

      <section>
        <h2>Versions</h2>
        <label class="check">
          <input type="checkbox" bind:checked={settings.showSnapshots} />
          Show snapshots when creating instances
        </label>
      </section>

      <div class="save">
        <button class="btn primary" type="submit" disabled={saving}>{saving ? "Saving…" : "Save changes"}</button>
        {#if message}
          <span class:ok={message.ok} class:bad={!message.ok}>{message.text}</span>
        {/if}
      </div>
    </form>
  {:else if message}
    <p class="alert">{message.text}</p>
  {/if}
</div>

<style>
  .scroll {
    height: 100%;
    overflow: auto;
    padding: 28px 32px 40px;
  }
  h1 {
    font-size: 26px;
    letter-spacing: -0.3px;
    margin-bottom: 22px;
  }
  form {
    display: grid;
    gap: 16px;
    max-width: 560px;
  }
  section {
    display: grid;
    gap: 14px;
    padding: 18px;
    border: 1px solid var(--line);
    border-radius: var(--radius);
    background: var(--card);
  }
  h2 {
    font-size: 15px;
  }
  .note {
    color: var(--muted);
  }
  .field b {
    float: right;
    color: var(--accent);
  }
  input[type="range"] {
    accent-color: var(--accent);
    width: 100%;
  }
  .save {
    display: flex;
    align-items: center;
    gap: 14px;
  }
  .ok {
    color: var(--ok);
  }
  .bad {
    color: var(--danger);
  }
</style>
