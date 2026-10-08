<script lang="ts">
  import { onMount } from "svelte";
  import { api, errorMessage, type CurseForgeStatus, type Settings } from "$lib/api";
  import { account } from "$lib/account.svelte";
  import LoginDialog from "$lib/components/LoginDialog.svelte";
  import { ui } from "$lib/ui.svelte";

  ui.setCrumbs({ label: "Settings" });

  let settings = $state<Settings | null>(null);
  let saving = $state(false);
  let message = $state<{ ok: boolean; text: string } | null>(null);
  let signingIn = $state(false);
  let cfStatus = $state<CurseForgeStatus | null>(null);
  let accountError = $state("");

  async function choose(uuid: string | null) {
    accountError = "";
    try {
      account.list = await api.setActiveAccount(uuid);
    } catch (e) {
      accountError = errorMessage(e);
    }
  }

  async function remove(uuid: string) {
    accountError = "";
    try {
      account.list = await api.removeAccount(uuid);
    } catch (e) {
      accountError = errorMessage(e);
    }
  }

  onMount(async () => {
    account.refresh();
    try {
      settings = await api.getSettings();
      cfStatus = await api.curseforgeStatus();
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
      cfStatus = await api.curseforgeStatus();
      if (settings.curseforgeApiKey) {
        try {
          await api.checkCurseforgeKey();
          message = { ok: true, text: "Saved. Your CurseForge key works." };
        } catch (err) {
          message = { ok: false, text: `Saved, but ${errorMessage(err)}` };
        }
      } else {
        message = { ok: true, text: "Saved." };
      }
    } catch (err) {
      message = { ok: false, text: errorMessage(err) };
    } finally {
      saving = false;
    }
  }
</script>

<div class="page">
<div class="page-inner narrow">
  <h1 class="page-title">Settings</h1>

  <section class="accounts">
    <div class="section-head">
      <h2>Accounts</h2>
      <button class="btn" onclick={() => (signingIn = true)}>Add Microsoft account</button>
    </div>
    <div class="account-list" role="radiogroup" aria-label="Play as">
      {#each account.list.accounts as a (a.uuid)}
        <div class="account" class:selected={account.list.active === a.uuid}>
          <label>
            <input type="radio" name="account" checked={account.list.active === a.uuid} onchange={() => choose(a.uuid)} />
            <img src="https://mc-heads.net/avatar/{a.uuid}/32" alt="" width="32" height="32" />
            <span><b>{a.username}</b><small>Microsoft account</small></span>
          </label>
          <button class="btn ghost" onclick={() => remove(a.uuid)}>Remove</button>
        </div>
      {/each}
      <div class="account" class:selected={account.list.active === null}>
        <label>
          <input type="radio" name="account" checked={account.list.active === null} onchange={() => choose(null)} />
          <span class="offline-icon">?</span>
          <span><b>Offline</b><small>Singleplayer and offline servers only</small></span>
        </label>
      </div>
    </div>
    {#if accountError}<p class="alert">{accountError}</p>{/if}
  </section>

  {#if settings}
    <form onsubmit={save}>
      <section>
        <h2>Offline mode</h2>
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
        <label class="field">
          <span>Java arguments</span>
          <textarea
            class="input mono"
            rows="2"
            bind:value={settings.javaArgs}
            placeholder="-XX:+UseG1GC"
            spellcheck="false"
          ></textarea>
          <small>Added to every instance. An instance's own arguments come after these.</small>
        </label>
      </section>

      <section>
        <h2>CurseForge</h2>
        <label class="field">
          <span>API key</span>
          <input
            class="input mono"
            type="password"
            bind:value={settings.curseforgeApiKey}
            placeholder="Paste your key from console.curseforge.com"
            autocomplete="off"
            spellcheck="false"
          />
          <small>
            {cfStatus?.managed
              ? "This build has a key built in. A key pasted here is used instead."
              : "Needed to browse and install from CurseForge. Stored only on this computer."}
          </small>
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
</div>

<LoginDialog
  open={signingIn}
  onclose={() => (signingIn = false)}
  onsignedin={async () => {
    signingIn = false;
    await account.refresh();
  }}
/>

<style>
  .narrow {
    max-width: 720px;
  }
  .mono {
    font-family: var(--mono);
    font-size: 13px;
  }
  form {
    display: grid;
    gap: 16px;
  }
  .section-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  .account-list {
    display: grid;
    gap: 6px;
  }
  .account {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 8px 10px;
    border-radius: 10px;
    background: var(--raised-2);
  }
  .account.selected {
    box-shadow: inset 0 0 0 1.5px var(--accent);
  }
  .account label {
    display: flex;
    align-items: center;
    gap: 10px;
    flex: 1;
    cursor: pointer;
  }
  .account input {
    accent-color: var(--accent);
  }
  .account img,
  .offline-icon {
    width: 32px;
    height: 32px;
    border-radius: 6px;
    image-rendering: pixelated;
    background: var(--card-hover);
  }
  .offline-icon {
    display: grid;
    place-items: center;
    color: var(--muted);
    font-weight: 700;
  }
  .account span:last-child {
    display: grid;
  }
  .account small {
    color: var(--faint);
  }
  section {
    display: grid;
    gap: 14px;
    padding: 18px;
    border-radius: var(--radius);
    background: var(--raised);
  }
  h2 {
    font-size: 15px;
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
