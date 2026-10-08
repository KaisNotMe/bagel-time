// Which instances a project's versions fit. Mirrors `Target::accepts` in
// bagel-core, so what the UI offers is what installing will accept.

import type { ContentKind, Loader, ProjectVersion } from "./api";

export const ALL_LOADERS: Loader[] = ["vanilla", "fabric", "quilt", "forge", "neoforge"];

/** Loader tags a version may carry to fit an instance with this loader, or null if the kind can't go in at all. */
export function acceptedLoaders(kind: ContentKind, loader: Loader): string[] | null {
  if (kind === "resourcepack") return ["minecraft"];
  if (loader === "vanilla") return null;
  // Iris runs both Iris and OptiFine shader packs.
  if (kind === "shader") return ["iris", "optifine"];
  return loader === "quilt" ? ["quilt", "fabric"] : [loader];
}

export function versionFits(v: ProjectVersion, kind: ContentKind, loader: Loader, minecraft: string): boolean {
  const accepted = acceptedLoaders(kind, loader);
  if (!accepted) return false;
  return v.gameVersions.includes(minecraft) && (v.loaders.length === 0 || v.loaders.some((l) => accepted.includes(l)));
}

/** For each loader, the Minecraft versions some project version fits. Loaders with none are left out. */
export function supportedGames(versions: ProjectVersion[], kind: ContentKind): Map<Loader, Set<string>> {
  const out = new Map<Loader, Set<string>>();
  for (const loader of ALL_LOADERS) {
    const accepted = acceptedLoaders(kind, loader);
    if (!accepted) continue;
    const games = new Set<string>();
    for (const v of versions) {
      if (v.loaders.length === 0 || v.loaders.some((l) => accepted.includes(l))) {
        for (const g of v.gameVersions) games.add(g);
      }
    }
    if (games.size > 0) out.set(loader, games);
  }
  return out;
}
