<script lang="ts">
  import { goto } from "$app/navigation";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { account } from "$lib/account.svelte";
  import { api, errorMessage, LOADER_NAMES, type Loader } from "$lib/api";
  import { hosting } from "$lib/hosting.svelte";
  import { instances } from "$lib/instances.svelte";
  import Icon from "./Icon.svelte";
  import InstanceIcon from "./InstanceIcon.svelte";
  import Modal from "./Modal.svelte";
  import PlayitConnect from "./PlayitConnect.svelte";
  import VersionPicker from "./VersionPicker.svelte";

  let { showSnapshots }: { showSnapshots: boolean } = $props();

  let mode = $state<"instance" | "fresh">("instance");
  let fromInstance = $state<string | null>(null);
  let name = $state("");
  let version = $state("");
  let loader = $state<Loader>("vanilla");
  let loaderVersion = $state("");
  let versionValid = $state(false);
  let tunnel = $state(true);
  let onlineMode = $state(false);
  let eula = $state(false);
  let creating = $state(false);
  let error = $state("");

  let open = $derived(hosting.dialog.open);
  let picked = $derived(fromInstance ? instances.get(fromInstance) : undefined);
  let placeholder = $derived(
    mode === "instance" ? (picked ? `${picked.name} server` : "My server") : version ? `${version} server` : "My server",
  );
  let valid = $derived(eula && (mode === "instance" ? !!picked : versionValid));

  // Reset each time it opens.
  $effect(() => {
    if (!open) return;
    error = "";
    eula = false;
    name = "";
    tunnel = true;
    // Offline players can only join servers that don't check accounts.
    onlineMode = account.list.active !== null;
    fromInstance = hosting.dialog.fromInstance ?? instances.list[0]?.id ?? null;
    mode = instances.list.length > 0 ? "instance" : "fresh";
    hosting.checkPlayit();
  });

  function close() {
    hosting.dialog = { open: false, fromInstance: null };
  }

  async function create(e: SubmitEvent) {
    e.preventDefault();
    if (!valid) return;
    creating = true;
    error = "";
    try {
      const game =
        mode === "instance" && picked
          ? { gameVersion: picked.gameVersion, loader: picked.loader, loaderVersion: picked.loaderVersion }
          : { gameVersion: version, loader, loaderVersion: loader === "vanilla" ? null : loaderVersion };
      const created = await api.createHostedServer({
        name: name.trim() || placeholder,
        ...game,
        onlineMode,
        tunnel,
        acceptEula: eula,
        fromInstance: mode === "instance" ? (picked?.id ?? null) : null,
      });
      if (created.skipped.length) hosting.skipped[created.server.id] = created.skipped;
      await hosting.refresh();
      close();
      await goto(`/hosted?id=${encodeURIComponent(created.server.id)}`);
      // Straight into it: that's the point.
      hosting.start(created.server.id);
    } catch (err) {
      error = errorMessage(err);
    } finally {
      creating = false;
    }
  }
</script>

<Modal {open} title="Host a server" onclose={close} width={560}>
  <form id="host-server" class="form" onsubmit={create}>
    <div class="segmented" role="radiogroup" aria-label="Start from">
      <button type="button" role="radio" aria-checked={mode === "instance"} class:selected={mode === "instance"}
        disabled={instances.list.length === 0} onclick={() => (mode = "instance")}>Same as one of my instances</button>
      <button type="button" role="radio" aria-checked={mode === "fresh"} class:selected={mode === "fresh"}
        onclick={() => (mode = "fresh")}>New server</button>
    </div>

    {#if mode === "instance"}
      <p class="muted small">
        The server gets the same Minecraft version and loader, plus the instance's mods and configs (mods that only
        work in the game client are left out). Your friends play with the same instance.
      </p>
      <div class="instances">
        {#each instances.list as inst (inst.id)}
          <button type="button" class="inst" class:selected={fromInstance === inst.id} onclick={() => (fromInstance = inst.id)}>
            <InstanceIcon instance={inst} size={34} />
            <span class="inst-text">
              <b>{inst.name}</b>
              <small>{LOADER_NAMES[inst.loader]} {inst.gameVersion}</small>
            </span>
            {#if fromInstance === inst.id}<Icon name="check" size={16} />{/if}
          </button>
        {/each}
      </div>
    {:else}
      <VersionPicker active={open && mode === "fresh"} {showSnapshots} bind:version bind:loader bind:loaderVersion bind:valid={versionValid} />
    {/if}

    <label class="field">
      <span>Server name</span>
      <input class="input" bind:value={name} {placeholder} maxlength="60" />
    </label>

    <label class="check">
      <input type="checkbox" bind:checked={tunnel} />
      <span>Let friends join from anywhere <small class="muted">(free playit.gg tunnel, no port forwarding)</small></span>
    </label>
    {#if tunnel && hosting.playitConnected === false}
      <PlayitConnect />
    {/if}

    <label class="check">
      <input type="checkbox" bind:checked={onlineMode} />
      <span>
        Only players signed in with a Microsoft account
        <small class="muted">
          {account.list.active
            ? "(recommended: stops people joining under someone else's name)"
            : "(leave off while you play offline, or you can't join your own server)"}
        </small>
      </span>
    </label>

    <label class="check eula">
      <input type="checkbox" bind:checked={eula} />
      <span>
        I agree to the
        <button type="button" class="link" onclick={() => openUrl("https://aka.ms/MinecraftEULA")}>Minecraft EULA</button>
        <small class="muted">(Mojang requires this to run a server)</small>
      </span>
    </label>

    {#if error}<p class="alert">{error}</p>{/if}
  </form>

  {#snippet footer()}
    <button class="btn ghost" type="button" onclick={close}>Cancel</button>
    <button class="btn primary" type="submit" form="host-server" disabled={!valid || creating}>
      <Icon name="play" size={14} fill />
      {creating ? "Creating…" : "Create and start"}
    </button>
  {/snippet}
</Modal>

<style>
  .form {
    display: grid;
    gap: 14px;
  }
  .small {
    font-size: 13px;
  }
  .segmented {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 4px;
    padding: 4px;
    border-radius: 12px;
    background: var(--raised-2);
  }
  .segmented button {
    height: 32px;
    border: 0;
    border-radius: 8px;
    background: none;
    color: var(--muted);
    font-weight: 600;
    cursor: pointer;
  }
  .segmented button.selected {
    background: var(--raised-3);
    color: var(--text);
  }
  .segmented button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .instances {
    display: grid;
    gap: 4px;
    max-height: 210px;
    overflow: auto;
  }
  .inst {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 8px;
    border: 1px solid transparent;
    border-radius: 10px;
    background: none;
    color: var(--text);
    text-align: left;
    cursor: pointer;
  }
  .inst:hover {
    background: var(--raised-2);
  }
  .inst.selected {
    border-color: var(--accent, var(--primary));
    background: var(--raised-2);
  }
  .inst-text {
    flex: 1;
    display: grid;
    min-width: 0;
  }
  .inst-text small {
    color: var(--muted);
  }
  .check {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    font-size: 14px;
    cursor: pointer;
  }
  .check input {
    margin-top: 3px;
  }
  .check small {
    font-size: 12.5px;
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
