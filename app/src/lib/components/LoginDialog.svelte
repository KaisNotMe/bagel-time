<script lang="ts">
  import { onDestroy } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { api, errorMessage, type AccountSummary, type DeviceLogin } from "$lib/api";
  import Modal from "./Modal.svelte";

  type Props = {
    open: boolean;
    onclose: () => void;
    onsignedin: (account: AccountSummary) => void;
  };
  let { open, onclose, onsignedin }: Props = $props();

  let login = $state<DeviceLogin | null>(null);
  let error = $state("");
  let starting = $state(false);
  let copied = $state(false);
  let unlisten: UnlistenFn | null = null;

  $effect(() => {
    if (open) start();
  });

  async function start() {
    login = null;
    error = "";
    starting = true;
    try {
      unlisten ??= await listen<{ account: AccountSummary | null; error: string | null }>(
        "login-finished",
        ({ payload }) => {
          if (payload.account) {
            login = null;
            onsignedin(payload.account);
          } else {
            error = payload.error ?? "Sign-in failed.";
            login = null;
          }
        },
      );
      login = await api.startLogin();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      starting = false;
    }
  }

  async function copyAndOpen() {
    if (!login) return;
    await navigator.clipboard.writeText(login.userCode);
    copied = true;
    setTimeout(() => (copied = false), 2000);
    try {
      await api.openLoginPage();
    } catch (e) {
      error = errorMessage(e);
    }
  }

  function close() {
    if (login) api.cancelLogin();
    login = null;
    onclose();
  }

  onDestroy(() => unlisten?.());
</script>

<Modal {open} title="Sign in with Microsoft" onclose={close}>
  {#if login}
    <p class="muted">Enter this code on Microsoft's page.</p>
    <div class="code" aria-label="Sign-in code">{login.userCode}</div>
    <button class="btn primary wide" onclick={copyAndOpen}>
      {copied ? "Code copied, opening browser…" : "Copy code and open browser"}
    </button>
    <p class="hint">
      <span class="spinner" aria-hidden="true"></span> Waiting for you to finish at
      {login.verificationUri.replace(/^https?:\/\//, "")}
    </p>
  {:else if starting}
    <p class="muted">Getting a sign-in code…</p>
  {/if}
  {#if error}
    <p class="alert">{error}</p>
    <button class="btn" onclick={start}>Try again</button>
  {/if}

  {#snippet footer()}
    <button class="btn ghost" onclick={close}>Cancel</button>
  {/snippet}
</Modal>

<style>
  .muted {
    color: var(--muted);
  }
  .code {
    padding: 14px;
    border: 1px dashed var(--line-strong);
    border-radius: 10px;
    background: var(--bg);
    text-align: center;
    font-family: var(--mono);
    font-size: 28px;
    font-weight: 700;
    letter-spacing: 4px;
    user-select: all;
  }
  .wide {
    width: 100%;
    height: 38px;
  }
  .hint {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--faint);
    font-size: 12.5px;
  }
  .spinner {
    width: 12px;
    height: 12px;
    border: 2px solid var(--line-strong);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
