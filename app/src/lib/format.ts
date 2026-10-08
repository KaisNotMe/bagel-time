export function relativeTime(unixSeconds: number | null): string {
  if (!unixSeconds) return "Never played";
  const diff = Date.now() / 1000 - unixSeconds;
  if (diff < 60) return "Played just now";
  if (diff < 3600) return `Played ${Math.floor(diff / 60)} min ago`;
  if (diff < 86400) return `Played ${Math.floor(diff / 3600)} h ago`;
  const days = Math.floor(diff / 86400);
  if (days === 1) return "Played yesterday";
  if (days < 30) return `Played ${days} days ago`;
  return `Played ${new Date(unixSeconds * 1000).toLocaleDateString()}`;
}

/** A stable colored tile with initials, derived from the instance name. */
export function iconFor(name: string): { initials: string; background: string } {
  let hash = 0;
  for (const ch of name) hash = (hash * 31 + ch.codePointAt(0)!) >>> 0;
  const hue = hash % 360;
  const initials =
    name
      .split(/[\s\-_.]+/)
      .filter(Boolean)
      .slice(0, 2)
      .map((w) => w[0]!.toUpperCase())
      .join("") || "?";
  return {
    initials,
    background: `linear-gradient(140deg, hsl(${hue} 62% 56%), hsl(${(hue + 35) % 360} 58% 34%))`,
  };
}

export function logTime(timestamp: number | null): string {
  if (!timestamp) return "";
  return new Date(timestamp).toLocaleTimeString([], { hour12: false });
}

const compact = new Intl.NumberFormat(undefined, { notation: "compact", maximumFractionDigits: 1 });

/** 238896364 -> "238.9M". */
export function compactNumber(n: number): string {
  return compact.format(n);
}
