<script lang="ts">
  let { size = 28 }: { size?: number } = $props();
</script>

<svg width={size} height={size} viewBox="0 0 32 32" aria-hidden="true">
  <defs>
    <linearGradient id="bagel-crust" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#ffc45e" />
      <stop offset="1" stop-color="#c46f1c" />
    </linearGradient>
  </defs>
  <circle cx="16" cy="16" r="10.5" fill="none" stroke="url(#bagel-crust)" stroke-width="9" />
  <g fill="#fff4dc">
    <ellipse cx="10" cy="8.5" rx="1.1" ry="0.6" transform="rotate(-30 10 8.5)" />
    <ellipse cx="19" cy="7" rx="1.1" ry="0.6" transform="rotate(20 19 7)" />
    <ellipse cx="25" cy="13" rx="1.1" ry="0.6" transform="rotate(70 25 13)" />
    <ellipse cx="7" cy="17" rx="1.1" ry="0.6" transform="rotate(-80 7 17)" />
    <ellipse cx="23.5" cy="22" rx="1.1" ry="0.6" transform="rotate(-40 23.5 22)" />
    <ellipse cx="13" cy="25" rx="1.1" ry="0.6" transform="rotate(10 13 25)" />
  </g>
</svg>
