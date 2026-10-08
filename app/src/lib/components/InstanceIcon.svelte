<script lang="ts">
  import { fileUrl, type Instance } from "$lib/api";
  import { iconFor } from "$lib/format";
  import { instances } from "$lib/instances.svelte";

  type Props = { instance: Pick<Instance, "id" | "name" | "iconPath">; size?: number; radius?: number };
  let { instance, size = 48, radius }: Props = $props();

  let failed = $state(false);
  let fallback = $derived(iconFor(instance.name));
  let src = $derived(
    instance.iconPath ? `${fileUrl(instance.iconPath)}?v=${instances.iconVersion[instance.id] ?? 0}` : null,
  );

  $effect(() => {
    void src;
    failed = false;
  });
</script>

<div
  class="icon"
  style:width="{size}px"
  style:height="{size}px"
  style:border-radius="{radius ?? Math.round(size * 0.24)}px"
  style:font-size="{Math.round(size * 0.36)}px"
  style:background={src && !failed ? "var(--raised-2)" : fallback.background}
>
  {#if src && !failed}
    <img {src} alt="" onerror={() => (failed = true)} />
  {:else}
    {fallback.initials}
  {/if}
</div>

<style>
  .icon {
    flex: none;
    display: grid;
    place-items: center;
    overflow: hidden;
    font-weight: 800;
    color: rgb(255 255 255 / 0.95);
    text-shadow: 0 1px 2px rgb(0 0 0 / 0.35);
  }
  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
</style>
