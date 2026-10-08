<script lang="ts">
  import type { Snippet } from "svelte";
  import Icon from "./Icon.svelte";

  type Props = {
    open: boolean;
    title: string;
    onclose: () => void;
    children: Snippet;
    footer?: Snippet;
    /** Max width in px. */
    width?: number;
  };
  let { open, title, onclose, children, footer, width = 460 }: Props = $props();
  let dialog = $state<HTMLDialogElement>();

  $effect(() => {
    if (!dialog) return;
    if (open && !dialog.open) dialog.showModal();
    else if (!open && dialog.open) dialog.close();
  });
</script>

<dialog
  bind:this={dialog}
  style:width="min({width}px, calc(100vw - 32px))"
  onclose={onclose}
  onclick={(e) => {
    if (e.target === dialog) onclose();
  }}
>
  <div class="modal">
    <header>
      <h2>{title}</h2>
      <button class="btn ghost sm square" aria-label="Close" onclick={onclose}><Icon name="x" size={16} /></button>
    </header>
    <div class="body">{@render children()}</div>
    {#if footer}
      <div class="footer">{@render footer()}</div>
    {/if}
  </div>
</dialog>

<style>
  dialog {
    padding: 0;
    border: 0;
    border-radius: var(--radius-lg);
    background: var(--raised);
    color: var(--text);
    max-height: calc(100vh - 48px);
    box-shadow: var(--shadow);
  }
  dialog::backdrop {
    background: rgb(8 6 5 / 0.65);
    backdrop-filter: blur(3px);
  }
  .modal {
    padding: 20px;
    display: grid;
    gap: 16px;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin: -4px -6px 0 0;
  }
  h2 {
    font-size: 18px;
  }
  .body {
    display: grid;
    gap: 14px;
    min-width: 0;
  }
  .footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
