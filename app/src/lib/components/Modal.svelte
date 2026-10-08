<script lang="ts">
  import type { Snippet } from "svelte";

  type Props = {
    open: boolean;
    title: string;
    onclose: () => void;
    children: Snippet;
    footer?: Snippet;
  };
  let { open, title, onclose, children, footer }: Props = $props();
  let dialog = $state<HTMLDialogElement>();

  $effect(() => {
    if (!dialog) return;
    if (open && !dialog.open) dialog.showModal();
    else if (!open && dialog.open) dialog.close();
  });
</script>

<dialog
  bind:this={dialog}
  onclose={onclose}
  onclick={(e) => {
    if (e.target === dialog) onclose();
  }}
>
  <div class="modal">
    <h2>{title}</h2>
    <div class="body">{@render children()}</div>
    {#if footer}
      <div class="footer">{@render footer()}</div>
    {/if}
  </div>
</dialog>

<style>
  dialog {
    padding: 0;
    border: 1px solid var(--line-strong);
    border-radius: 14px;
    background: var(--panel);
    color: var(--text);
    width: min(440px, calc(100vw - 32px));
    box-shadow: 0 24px 60px rgb(0 0 0 / 0.5);
  }
  dialog::backdrop {
    background: rgb(8 6 5 / 0.6);
    backdrop-filter: blur(2px);
  }
  .modal {
    padding: 20px;
    display: grid;
    gap: 16px;
  }
  h2 {
    font-size: 18px;
  }
  .body {
    display: grid;
    gap: 14px;
  }
  .footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
