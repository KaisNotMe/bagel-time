<script lang="ts">
  import { api, errorMessage, LOADER_NAMES, type Instance, type Loader } from "$lib/api";
  import { packs } from "$lib/packs.svelte";
  import Modal from "./Modal.svelte";
  import VersionPicker from "./VersionPicker.svelte";

  type Props = {
    open: boolean;
    showSnapshots: boolean;
    onclose: () => void;
    oncreated: (instance: Instance) => void;
  };
  let { open, showSnapshots, onclose, oncreated }: Props = $props();

  let name = $state("");
  let version = $state("");
  let loader = $state<Loader>("vanilla");
  let loaderVersion = $state("");
  let valid = $state(false);
  let creating = $state(false);
  let error = $state("");

  let placeholder = $derived(
    version ? (loader === "vanilla" ? version : `${LOADER_NAMES[loader]} ${version}`) : "My instance",
  );

  async function create(e: SubmitEvent) {
    e.preventDefault();
    creating = true;
    error = "";
    try {
      const instance = await api.createInstance(
        name.trim() || placeholder,
        version,
        loader,
        loader === "vanilla" ? null : loaderVersion,
      );
      name = "";
      oncreated(instance);
    } catch (err) {
      error = errorMessage(err);
    } finally {
      creating = false;
    }
  }
</script>

<Modal {open} title="New instance" {onclose}>
  <form id="new-instance" class="form" onsubmit={create}>
    <label class="field">
      <span>Name</span>
      <input class="input" bind:value={name} {placeholder} maxlength="60" />
    </label>
    <VersionPicker active={open} {showSnapshots} bind:version bind:loader bind:loaderVersion bind:valid />
    {#if error}<p class="alert">{error}</p>{/if}
  </form>

  {#snippet footer()}
    <button
      class="btn ghost import"
      type="button"
      disabled={packs.busy}
      onclick={() => {
        onclose();
        packs.pickFile();
      }}>Import .mrpack…</button
    >
    <button class="btn ghost" type="button" onclick={onclose}>Cancel</button>
    <button class="btn primary" type="submit" form="new-instance" disabled={!valid || creating}>
      {creating ? "Creating…" : "Create"}
    </button>
  {/snippet}
</Modal>

<style>
  .form {
    display: grid;
    gap: 14px;
  }
  .import {
    margin-right: auto;
  }
</style>
