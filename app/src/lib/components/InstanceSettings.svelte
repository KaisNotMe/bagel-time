<script lang="ts">
  import { untrack } from "svelte";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { api, errorMessage, LOADER_NAMES, type Instance, type Loader } from "$lib/api";
  import { games } from "$lib/games.svelte";
  import { instances } from "$lib/instances.svelte";
  import Icon, { type IconName } from "./Icon.svelte";
  import InstanceIcon from "./InstanceIcon.svelte";
  import Modal from "./Modal.svelte";
  import VersionPicker from "./VersionPicker.svelte";

  type Props = {
    open: boolean;
    instance: Instance;
    onclose: () => void;
    ondeleted: () => void;
    onduplicated: (copy: Instance) => void;
  };
  let { open, instance, onclose, ondeleted, onduplicated }: Props = $props();

  type Tab = "general" | "installation" | "java";
  const tabs: { id: Tab; label: string; icon: IconName }[] = [
    { id: "general", label: "General", icon: "info" },
    { id: "installation", label: "Installation", icon: "box" },
    { id: "java", label: "Java & memory", icon: "code" },
  ];
  let tab = $state<Tab>("general");

  let name = $state("");
  let customMemory = $state(false);
  let memoryMb = $state(4096);
  let javaArgs = $state("");
  let version = $state("");
  let loader = $state<Loader>("vanilla");
  let loaderVersion = $state("");
  let versionValid = $state(false);
  let busy = $state(false);
  let error = $state("");
  let saved = $state("");
  let confirmDelete = $state(false);

  let running = $derived(!!games.status[instance.id]);
  let versionChanged = $derived(
    version !== instance.gameVersion ||
      loader !== instance.loader ||
      (loader !== "vanilla" && loaderVersion !== (instance.loaderVersion ?? "")),
  );

  // Fresh values every time the dialog opens (not on every save).
  $effect(() => {
    if (open) untrack(reset);
  });

  function reset() {
    tab = "general";
    name = instance.name;
    customMemory = instance.memoryMb !== null;
    memoryMb = instance.memoryMb ?? 4096;
    javaArgs = instance.javaArgs ?? "";
    version = instance.gameVersion;
    loader = instance.loader;
    loaderVersion = instance.loaderVersion ?? "";
    error = "";
    saved = "";
    confirmDelete = false;
  }

  async function run(task: () => Promise<void>, done = "Saved") {
    busy = true;
    error = "";
    saved = "";
    try {
      await task();
      saved = done;
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }

  function update(i: Instance) {
    instances.replace(i);
  }

  const saveGeneral = () =>
    run(async () => {
      update(
        await api.updateInstance(instance.id, {
          name,
          memoryMb: instance.memoryMb,
          javaArgs: instance.javaArgs,
        }),
      );
    });

  const saveJava = () =>
    run(async () => {
      update(
        await api.updateInstance(instance.id, {
          name: instance.name,
          memoryMb: customMemory ? memoryMb : null,
          javaArgs: javaArgs.trim() || null,
        }),
      );
    });

  const changeVersion = () =>
    run(async () => {
      update(
        await api.changeInstanceVersion(instance.id, version, loader, loader === "vanilla" ? null : loaderVersion),
      );
    }, "Version changed");

  async function pickIcon() {
    const path = await openDialog({
      title: "Choose an icon",
      multiple: false,
      directory: false,
      filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp", "gif"] }],
    });
    if (typeof path !== "string") return;
    await run(async () => {
      update(await api.setInstanceIcon(instance.id, path));
      instances.iconChanged(instance.id);
    }, "Icon updated");
  }

  const removeIcon = () =>
    run(async () => {
      update(await api.clearInstanceIcon(instance.id));
      instances.iconChanged(instance.id);
    }, "Icon removed");

  const duplicate = () =>
    run(async () => {
      const copy = await api.duplicateInstance(instance.id, "");
      await instances.refresh();
      onduplicated(copy);
    }, "Duplicated");

  async function remove() {
    if (!confirmDelete) {
      confirmDelete = true;
      return;
    }
    await run(async () => {
      await api.deleteInstance(instance.id);
      await instances.refresh();
      ondeleted();
    }, "Deleted");
  }
</script>

<Modal {open} title="{instance.name} settings" {onclose} width={720}>
  <div class="layout">
    <nav class="side" aria-label="Settings sections">
      {#each tabs as t (t.id)}
        <button class:active={tab === t.id} onclick={() => ((tab = t.id), (error = ""), (saved = ""))}>
          <Icon name={t.icon} size={16} />
          {t.label}
        </button>
      {/each}
    </nav>

    <div class="content">
      {#if tab === "general"}
        <div class="icon-row">
          <InstanceIcon {instance} size={84} radius={18} />
          <div class="icon-actions">
            <button class="btn sm" disabled={busy} onclick={pickIcon}><Icon name="upload" size={15} /> Change icon</button>
            {#if instance.icon}
              <button class="btn ghost sm" disabled={busy} onclick={removeIcon}><Icon name="trash" size={15} /> Remove icon</button>
            {/if}
          </div>
        </div>

        <label class="field">
          <span>Name</span>
          <div class="inline">
            <input class="input grow" bind:value={name} maxlength="60" />
            <button class="btn primary" disabled={busy || !name.trim() || name === instance.name} onclick={saveGeneral}>
              Save
            </button>
          </div>
        </label>

        <div class="group">
          <div>
            <h3>Duplicate</h3>
            <p>Makes a full copy, worlds and mods included.</p>
          </div>
          <button class="btn" disabled={busy || running} onclick={duplicate}><Icon name="copyplus" size={15} /> Duplicate</button>
        </div>

        <div class="group danger">
          <div>
            <h3>Delete instance</h3>
            <p>Removes the instance and everything in its folder, including worlds. This can't be undone.</p>
          </div>
          <button class="btn {confirmDelete ? 'danger' : 'danger-soft'}" disabled={busy || running} onclick={remove}>
            <Icon name="trash" size={15} />
            {confirmDelete ? "Click again to delete" : "Delete"}
          </button>
        </div>
      {:else if tab === "installation"}
        <div class="current">
          <span class="faint">Currently</span>
          <b>
            {LOADER_NAMES[instance.loader]}
            {instance.gameVersion}{instance.loaderVersion ? ` · loader ${instance.loaderVersion}` : ""}
          </b>
        </div>
        <VersionPicker
          active={open && tab === "installation"}
          showSnapshots={false}
          bind:version
          bind:loader
          bind:loaderVersion
          bind:valid={versionValid}
        />
        {#if versionChanged}
          <p class="warning">
            Mods made for one version usually don't work on another, and going back to an older Minecraft version can
            damage worlds. Duplicate the instance first if you want a backup.
          </p>
        {/if}
        <div class="actions">
          <button class="btn primary" disabled={busy || running || !versionValid || !versionChanged} onclick={changeVersion}>
            Change version
          </button>
        </div>
      {:else}
        <label class="check">
          <input type="checkbox" bind:checked={customMemory} />
          Use custom memory for this instance
        </label>
        <label class="field" class:disabled={!customMemory}>
          <span>Maximum memory <b class="value">{(memoryMb / 1024).toFixed(1)} GB</b></span>
          <input type="range" min="1024" max="16384" step="512" bind:value={memoryMb} disabled={!customMemory} />
          <small>Off means the default from Settings is used.</small>
        </label>
        <label class="field">
          <span>Java arguments</span>
          <textarea
            class="input mono"
            rows="3"
            bind:value={javaArgs}
            placeholder="-XX:+UseG1GC -XX:MaxGCPauseMillis=50"
            spellcheck="false"
          ></textarea>
          <small>Added after the launcher's own arguments, separated by spaces.</small>
        </label>
        <div class="actions">
          <button class="btn primary" disabled={busy} onclick={saveJava}>Save</button>
        </div>
      {/if}

      {#if error}<p class="alert">{error}</p>{/if}
      {#if saved && !error}<p class="saved"><Icon name="check" size={15} /> {saved}</p>{/if}
    </div>
  </div>
</Modal>

<style>
  .layout {
    display: grid;
    grid-template-columns: 180px 1fr;
    gap: 20px;
    min-height: 380px;
  }
  .side {
    display: grid;
    align-content: start;
    gap: 2px;
  }
  .side button {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 38px;
    padding: 0 12px;
    border: 0;
    border-radius: 10px;
    background: none;
    color: var(--muted);
    font-weight: 600;
    text-align: left;
    cursor: pointer;
  }
  .side button:hover {
    background: var(--raised-2);
    color: var(--text);
  }
  .side button.active {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .content {
    display: flex;
    flex-direction: column;
    gap: 16px;
    min-width: 0;
  }
  .icon-row {
    display: flex;
    align-items: center;
    gap: 16px;
  }
  .icon-actions {
    display: grid;
    gap: 6px;
    justify-items: start;
  }
  .inline {
    display: flex;
    gap: 8px;
  }
  .grow {
    flex: 1;
  }
  .group {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 14px;
    border-radius: var(--radius);
    background: var(--raised-2);
  }
  .group h3 {
    font-size: 14px;
  }
  .group p {
    color: var(--muted);
    font-size: 13px;
  }
  .group.danger h3 {
    color: var(--danger);
  }
  .current {
    display: flex;
    gap: 8px;
    align-items: baseline;
  }
  .warning {
    padding: 10px 12px;
    border-radius: 10px;
    background: rgb(233 196 106 / 0.12);
    color: var(--warn);
    font-size: 13px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
  }
  .value {
    float: right;
    color: var(--accent);
  }
  input[type="range"] {
    accent-color: var(--accent);
    width: 100%;
  }
  .disabled {
    opacity: 0.55;
  }
  .mono {
    font-family: var(--mono);
    font-size: 13px;
  }
  .saved {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--ok);
  }
</style>
