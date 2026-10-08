<script lang="ts">
  import { iconFor } from "$lib/format";

  type Props = { url: string | null; name: string; size?: number };
  let { url, name, size = 44 }: Props = $props();

  let failed = $state(false);
  let fallback = $derived(iconFor(name));

  // A new URL deserves a fresh try.
  $effect(() => {
    void url;
    failed = false;
  });
</script>

{#if url && !failed}
  <img
    src={url}
    alt=""
    width={size}
    height={size}
    loading="lazy"
    style:width="{size}px"
    style:height="{size}px"
    onerror={() => (failed = true)}
  />
{:else}
  <div
    class="tile"
    style:width="{size}px"
    style:height="{size}px"
    style:font-size="{Math.round(size * 0.36)}px"
    style:background={fallback.background}
  >
    {fallback.initials}
  </div>
{/if}

<style>
  img,
  .tile {
    flex: none;
    border-radius: 10px;
  }
  img {
    object-fit: cover;
    background: var(--card-hover);
  }
  .tile {
    display: grid;
    place-items: center;
    font-weight: 800;
    color: rgb(255 255 255 / 0.95);
    text-shadow: 0 1px 2px rgb(0 0 0 / 0.35);
  }
</style>
