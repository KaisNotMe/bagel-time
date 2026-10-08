<script lang="ts">
  import Icon from "./Icon.svelte";

  let { address, small = false }: { address: string; small?: boolean } = $props();

  let copied = $state(false);
  let timer: ReturnType<typeof setTimeout> | null = null;

  async function copy(e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    try {
      await navigator.clipboard.writeText(address);
      copied = true;
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => (copied = false), 1500);
    } catch {
      // Clipboard blocked; the address is still on screen to copy by hand.
    }
  }
</script>

<button class="copy" class:small title="Copy address" onclick={copy}>
  <span class="addr">{address}</span>
  <Icon name={copied ? "check" : "copy"} size={small ? 13 : 15} />
  {#if copied}<span class="ok">Copied</span>{/if}
</button>

<style>
  .copy {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    max-width: 100%;
    padding: 6px 10px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--raised-2);
    color: var(--text);
    font-family: var(--mono);
    font-size: 14px;
    cursor: pointer;
  }
  .copy:hover {
    border-color: var(--line-strong);
  }
  .copy.small {
    padding: 3px 8px;
    font-size: 12px;
    gap: 6px;
  }
  .addr {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ok {
    font-family: inherit;
    color: var(--ok);
    font-size: 12px;
  }
</style>
