<script lang="ts">
  // Renders a server's message of the day with Minecraft `§` colour and
  // style codes (plus `§#rrggbb`, which the backend uses for hex colours).

  const COLORS: Record<string, string> = {
    "0": "#000000",
    "1": "#0000aa",
    "2": "#00aa00",
    "3": "#00aaaa",
    "4": "#aa0000",
    "5": "#aa00aa",
    "6": "#ffaa00",
    "7": "#aaaaaa",
    "8": "#555555",
    "9": "#5555ff",
    a: "#55ff55",
    b: "#55ffff",
    c: "#ff5555",
    d: "#ff55ff",
    e: "#ffff55",
    f: "#ffffff",
  };

  type Span = { text: string; color: string | null; bold: boolean; italic: boolean; underline: boolean; strike: boolean };

  let { text, lines = 2 }: { text: string; lines?: number } = $props();

  function parse(source: string): Span[][] {
    const out: Span[][] = [[]];
    let style: Omit<Span, "text"> = { color: null, bold: false, italic: false, underline: false, strike: false };
    let i = 0;
    let buf = "";
    const flush = () => {
      if (buf) out[out.length - 1].push({ text: buf, ...style });
      buf = "";
    };
    while (i < source.length) {
      const ch = source[i];
      if (ch === "\n") {
        flush();
        out.push([]);
        i++;
      } else if (ch === "§" && i + 1 < source.length) {
        flush();
        const code = source[i + 1].toLowerCase();
        if (code === "#" && /^[0-9a-f]{6}$/i.test(source.slice(i + 2, i + 8))) {
          style = { color: `#${source.slice(i + 2, i + 8)}`, bold: false, italic: false, underline: false, strike: false };
          i += 8;
          continue;
        }
        if (code in COLORS) style = { color: COLORS[code], bold: false, italic: false, underline: false, strike: false };
        else if (code === "l") style = { ...style, bold: true };
        else if (code === "o") style = { ...style, italic: true };
        else if (code === "n") style = { ...style, underline: true };
        else if (code === "m") style = { ...style, strike: true };
        else if (code === "r") style = { color: null, bold: false, italic: false, underline: false, strike: false };
        i += 2;
      } else {
        buf += ch;
        i++;
      }
    }
    flush();
    return out;
  }

  /** Servers pad lines with spaces to centre them in the game; drop that here. */
  function trimStart(line: Span[]): Span[] {
    const first = line.findIndex((s) => s.text.trim() !== "");
    if (first === -1) return [];
    return [{ ...line[first], text: line[first].text.trimStart() }, ...line.slice(first + 1)];
  }

  let parsed = $derived(parse(text).map(trimStart).slice(0, lines));
</script>

<span class="motd">
  {#each parsed as line, n (n)}
    <span class="line">
      {#each line as s, k (k)}<span
          style:color={s.color}
          class:b={s.bold}
          class:i={s.italic}
          class:u={s.underline}
          class:s={s.strike}>{s.text}</span
        >{/each}{#if line.length === 0}&nbsp;{/if}
    </span>
  {/each}
</span>

<style>
  .motd {
    display: grid;
    font-family: var(--mono, ui-monospace, monospace);
    font-size: 12px;
    line-height: 1.35;
    color: var(--muted);
  }
  .line {
    white-space: pre;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .b {
    font-weight: 700;
  }
  .i {
    font-style: italic;
  }
  .u {
    text-decoration: underline;
  }
  .s {
    text-decoration: line-through;
  }
  .u.s {
    text-decoration: underline line-through;
  }
</style>
