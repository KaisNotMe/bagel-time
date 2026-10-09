<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { page } from "$app/state";
  import {
    api,
    errorMessage,
    LOADER_NAMES,
    PROJECT_TYPE_NAMES,
    projectPageUrl,
    SOURCE_NAMES,
    type ContentKind,
    type GalleryImage,
    type ProjectDetails,
    type ProjectVersion,
    type Source,
    type TeamMember,
  } from "$lib/api";
  import Icon, { type IconName } from "$lib/components/Icon.svelte";
  import InstallButton from "$lib/components/InstallButton.svelte";
  import InstanceIcon from "$lib/components/InstanceIcon.svelte";
  import Markdown from "$lib/components/Markdown.svelte";
  import Modal from "$lib/components/Modal.svelte";
  import ProjectIcon from "$lib/components/ProjectIcon.svelte";
  import { compactNumber } from "$lib/format";
  import { acceptedLoaders, versionFits } from "$lib/fit";
  import { instances } from "$lib/instances.svelte";
  import { ui } from "$lib/ui.svelte";

  type Tab = "description" | "gallery" | "versions";

  let source = $derived<Source>(page.url.searchParams.get("source") === "curseforge" ? "curseforge" : "modrinth");
  let id = $derived(page.url.searchParams.get("id") ?? "");
  let instanceId = $derived(page.url.searchParams.get("instance"));
  let target = $derived(instanceId ? instances.get(instanceId) : undefined);

  let project = $state<ProjectDetails | null>(null);
  let members = $state<TeamMember[]>([]);
  let versions = $state<ProjectVersion[]>([]);
  let versionsLoaded = $state(false);
  let error = $state("");
  let tab = $state<Tab>("description");
  let onlyCompatible = $state(true);
  let viewing = $state<GalleryImage | null>(null);
  let installedIds = $state<Set<string>>(new Set());

  let gallery = $derived(project ? [...project.gallery].sort((a, b) => a.ordering - b.ordering) : []);
  let isContent = $derived(project !== null && project.projectType !== "modpack");
  /** Why a version won't work in the instance, or null if it will (same rule as installing). */
  function versionProblem(v: ProjectVersion): string | null {
    if (!target || !project || project.projectType === "modpack") return null;
    const kind = project.projectType;
    if (!acceptedLoaders(kind, target.loader)) return "Needs a mod loader";
    return versionFits(v, kind, target.loader, target.gameVersion)
      ? null
      : `Not made for ${LOADER_NAMES[target.loader]} ${target.gameVersion}`;
  }
  let shownVersions = $derived(target && onlyCompatible ? versions.filter((v) => !versionProblem(v)) : versions);
  /** Why the project as a whole can't go into the instance. */
  let projectProblem = $state<string | null>(null);
  let installed = $derived(!!project && installedIds.has(project.id));
  let pageUrl = $derived(
    project ? (project.websiteUrl ?? projectPageUrl(source, project.projectType, project.slug)) : "",
  );
  let blockedUrl = $derived(project?.downloadsBlocked ? pageUrl : null);
  let releaseVersions = $derived(
    project ? project.gameVersions.filter((v) => /^\d+\.\d+(\.\d+)?$/.test(v)).reverse() : [],
  );

  async function loadInstalled() {
    if (!instanceId) return;
    try {
      const list = await api.listContent(instanceId);
      installedIds = new Set(list.map((c) => c.projectId).filter((p): p is string => !!p));
    } catch {
      // Not important enough to show.
    }
  }

  $effect(() => {
    if (!id) return;
    const s = source;
    project = null;
    members = [];
    versions = [];
    versionsLoaded = false;
    error = "";
    tab = "description";
    api
      .getProject(s, id)
      .then((p) => (project = p))
      .catch((e) => (error = errorMessage(e)));
    api.getProjectMembers(s, id).then((m) => (members = m)).catch(() => {});
    api
      .getProjectVersions(s, id)
      .then((v) => (versions = v))
      .catch(() => {})
      .finally(() => (versionsLoaded = true));
    loadInstalled();
  });

  $effect(() => {
    projectProblem = null;
    if (!project || !isContent || !target) return;
    const instance = target.id;
    api
      .checkContentFit(project.projectType as ContentKind, source, project.id)
      .then((f) => (projectProblem = f[instance] && !f[instance].fits ? f[instance].reason : null))
      .catch(() => {});
  });

  $effect(() => {
    const type = project?.projectType;
    ui.setCrumbs(
      { label: "Discover", href: `/discover?source=${source}` },
      ...(type
        ? [
            {
              label: PROJECT_TYPE_NAMES[type].many,
              href: `/discover?source=${source}&type=${type}${instanceId ? `&instance=${encodeURIComponent(instanceId)}` : ""}`,
            },
          ]
        : []),
      { label: project?.title ?? "Project" },
    );
  });

  function date(iso: string) {
    return new Date(iso).toLocaleDateString(undefined, { dateStyle: "medium" });
  }

  function ago(iso: string) {
    const days = Math.floor((Date.now() - new Date(iso).getTime()) / 86_400_000);
    if (days < 1) return "today";
    if (days === 1) return "yesterday";
    if (days < 30) return `${days} days ago`;
    if (days < 365) return `${Math.floor(days / 30)} months ago`;
    return `${Math.floor(days / 365)} years ago`;
  }

  function side(value: string | null) {
    return value === "required" ? "Required" : value === "optional" ? "Optional" : value === "unsupported" ? "Not used" : "Unknown";
  }

  let links = $derived<{ label: string; url: string; icon: IconName }[]>(
    project
      ? [
          { label: "Source code", url: project.sourceUrl ?? "", icon: "code" as IconName },
          { label: "Issues", url: project.issuesUrl ?? "", icon: "bug" as IconName },
          { label: "Wiki", url: project.wikiUrl ?? "", icon: "book" as IconName },
          { label: "Discord", url: project.discordUrl ?? "", icon: "chat" as IconName },
        ].filter((l) => l.url)
      : [],
  );
</script>

<div class="page">
  <div class="page-inner">
    {#if error}
      <p class="alert">Couldn't load this project: {error}</p>
    {:else if !project}
      <div class="hero card skeleton"></div>
    {:else}
      <header class="hero card">
        <ProjectIcon url={project.iconUrl} name={project.title} size={96} />
        <div class="info">
          <h1>{project.title}</h1>
          <p class="desc">{project.description}</p>
          <div class="meta">
            <span><Icon name="download" size={15} /> {compactNumber(project.downloads)} downloads</span>
            <span><Icon name="heart" size={15} /> {compactNumber(project.followers)} followers</span>
            <span><Icon name="clock" size={15} /> Updated {ago(project.updated)}</span>
          </div>
          <div class="tags">
            {#each [...project.categories, ...project.additionalCategories] as c (c)}
              <span class="tag">{c.replace(/-/g, " ")}</span>
            {/each}
          </div>
        </div>
        <div class="actions">
          <InstallButton
            {source}
            {blockedUrl}
            projectId={project.id}
            title={project.title}
            projectType={project.projectType}
            instanceId={isContent ? (target?.id ?? null) : null}
            installed={isContent && installed}
            unavailable={isContent && !installed ? projectProblem : null}
            oninstalled={loadInstalled}
          />
          <button class="btn" onclick={() => openUrl(pageUrl)}>
            <Icon name="external" size={15} /> {SOURCE_NAMES[source]}
          </button>
        </div>
      </header>

      {#if target && isContent}
        <div class="target card">
          <InstanceIcon instance={target} size={30} />
          <span>Installing to <b>{target.name}</b> · {LOADER_NAMES[target.loader]} {target.gameVersion}</span>
        </div>
      {/if}

      <nav class="pills">
        <button class="pill" class:active={tab === "description"} onclick={() => (tab = "description")}>
          <Icon name="file" size={15} /> Description
        </button>
        {#if gallery.length > 0}
          <button class="pill" class:active={tab === "gallery"} onclick={() => (tab = "gallery")}>
            <Icon name="image" size={15} /> Gallery <span class="count">{gallery.length}</span>
          </button>
        {/if}
        <button class="pill" class:active={tab === "versions"} onclick={() => (tab = "versions")}>
          <Icon name="list" size={15} /> Versions <span class="count">{versionsLoaded ? versions.length : "…"}</span>
        </button>
      </nav>

      <div class="columns">
        <div class="main card">
          {#if tab === "description"}
            <Markdown source={project.body} html={source === "curseforge"} />
          {:else if tab === "gallery"}
            <div class="gallery">
              {#each gallery as img (img.url)}
                <button class="shot" onclick={() => (viewing = img)}>
                  <img src={img.url} alt={img.title ?? ""} loading="lazy" />
                  {#if img.title}<span>{img.title}</span>{/if}
                </button>
              {/each}
            </div>
          {:else}
            {#if target && isContent}
              <label class="check only">
                <input type="checkbox" bind:checked={onlyCompatible} />
                Only versions for {target.name} ({LOADER_NAMES[target.loader]} {target.gameVersion})
              </label>
            {/if}
            <div class="versions">
              {#each shownVersions.slice(0, 100) as v (v.id)}
                {@const problem = isContent ? versionProblem(v) : null}
                <div class="version" class:off={problem}>
                  <span class="vtype {v.versionType}" title={v.versionType}>{v.versionType[0].toUpperCase()}</span>
                  <div class="vinfo">
                    <b>{v.name}</b>
                    <small>{v.versionNumber}</small>
                  </div>
                  <div class="vmeta">
                    <span>{v.loaders.map((l) => l[0].toUpperCase() + l.slice(1)).join(", ")}</span>
                    <small>
                      {v.gameVersions.length > 3
                        ? `${v.gameVersions.slice(-3).reverse().join(", ")} +${v.gameVersions.length - 3}`
                        : [...v.gameVersions].reverse().join(", ")}
                    </small>
                  </div>
                  <div class="vstats">
                    <span>{compactNumber(v.downloads)}</span>
                    <small>{date(v.datePublished)}</small>
                  </div>
                  <InstallButton
                    {source}
                    blockedUrl={v.files[0] && !v.files[0].url ? `${pageUrl}/files/${v.id}` : null}
                    projectId={project.id}
                    title={`${project.title} ${v.versionNumber}`}
                    projectType={project.projectType}
                    versionId={v.id}
                    instanceId={isContent ? (target?.id ?? null) : null}
                    unavailable={problem}
                    small
                    oninstalled={loadInstalled}
                  />
                </div>
              {:else}
                <p class="empty">
                  {!versionsLoaded
                    ? "Loading versions…"
                    : target && onlyCompatible && versions.length > 0
                      ? `No versions work in ${target.name}. Untick the box above to see them all.`
                      : "No versions match."}
                </p>
              {/each}
            </div>
          {/if}
        </div>

        <aside class="side">
          <section class="card">
            <h3>Compatibility</h3>
            <div class="kv">
              <span>Minecraft</span>
              <div class="chips">
                {#each releaseVersions.slice(0, 8) as v (v)}<span class="tag">{v}</span>{/each}
                {#if releaseVersions.length > 8}<span class="tag">+{releaseVersions.length - 8} more</span>{/if}
                {#if releaseVersions.length === 0}<span class="faint">Snapshots only</span>{/if}
              </div>
            </div>
            {#if project.loaders.length}
              <div class="kv">
                <span>Platforms</span>
                <div class="chips">
                  {#each project.loaders as l (l)}<span class="tag">{l[0].toUpperCase() + l.slice(1)}</span>{/each}
                </div>
              </div>
            {/if}
            {#if project.projectType !== "resourcepack" && project.projectType !== "shader"}
              <div class="kv two">
                <span>Client</span><b>{side(project.clientSide)}</b>
                <span>Server</span><b>{side(project.serverSide)}</b>
              </div>
            {/if}
          </section>

          {#if links.length}
            <section class="card">
              <h3>Links</h3>
              {#each links as l (l.url)}
                <button class="link" onclick={() => openUrl(l.url)}>
                  <Icon name={l.icon} size={16} />
                  {l.label}
                  <Icon name="external" size={13} />
                </button>
              {/each}
            </section>
          {/if}

          {#if members.length}
            <section class="card">
              <h3>Creators</h3>
              {#each members as m (m.username)}
                <button
                  class="member"
                  onclick={() =>
                    openUrl(
                      source === "curseforge"
                        ? `https://www.curseforge.com/members/${m.username}`
                        : `https://modrinth.com/user/${m.username}`,
                    )}
                >
                  {#if m.avatarUrl}
                    <img src={m.avatarUrl} alt="" width="32" height="32" />
                  {:else}
                    <span class="avatar"><Icon name="user" size={16} /></span>
                  {/if}
                  <span class="who"><b>{m.username}</b><small>{m.role}</small></span>
                </button>
              {/each}
            </section>
          {/if}

          <section class="card">
            <h3>Details</h3>
            <div class="kv two">
              {#if project.license}<span>License</span><b>{project.license.name || project.license.id}</b>{/if}
              <span>Published</span><b>{date(project.published)}</b>
              <span>Updated</span><b>{date(project.updated)}</b>
            </div>
          </section>
        </aside>
      </div>
    {/if}
  </div>
</div>

<Modal open={viewing !== null} title={viewing?.title ?? "Gallery"} onclose={() => (viewing = null)} width={1100}>
  {#if viewing}
    <img class="full" src={viewing.url} alt={viewing.title ?? ""} />
    {#if viewing.description}<p class="muted">{viewing.description}</p>{/if}
  {/if}
</Modal>

<style>
  .hero {
    display: flex;
    gap: 20px;
    padding: 20px;
    align-items: flex-start;
  }
  .hero.skeleton {
    height: 160px;
  }
  .info {
    flex: 1;
    min-width: 0;
    display: grid;
    gap: 8px;
  }
  h1 {
    font-size: 26px;
    letter-spacing: -0.4px;
  }
  .desc {
    color: var(--muted);
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: 16px;
    color: var(--muted);
    font-size: 13px;
    font-weight: 600;
  }
  .meta span {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .tags .tag {
    text-transform: capitalize;
  }
  .actions {
    display: flex;
    gap: 8px;
    align-items: flex-start;
  }
  .target {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
    background: var(--accent-soft);
    font-size: 13px;
  }
  .columns {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 290px;
    gap: 16px;
    align-items: start;
  }
  .main {
    padding: 22px;
    min-width: 0;
  }
  .side {
    display: grid;
    gap: 12px;
  }
  .side section {
    display: grid;
    gap: 10px;
    padding: 16px;
  }
  .side h3 {
    font-size: 14px;
  }
  .kv {
    display: grid;
    gap: 6px;
  }
  .kv > span {
    color: var(--faint);
    font-size: 12.5px;
    font-weight: 600;
  }
  .kv.two {
    grid-template-columns: auto 1fr;
    column-gap: 14px;
    row-gap: 6px;
  }
  .kv.two b {
    font-size: 13px;
    text-align: right;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .link,
  .member {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 8px;
    margin: 0 -8px;
    border: 0;
    border-radius: 8px;
    background: none;
    color: var(--text);
    font-weight: 600;
    text-align: left;
    cursor: pointer;
  }
  .link:hover,
  .member:hover {
    background: var(--raised-2);
  }
  .link :global(svg:last-child) {
    margin-left: auto;
    color: var(--faint);
  }
  .member img,
  .avatar {
    width: 32px;
    height: 32px;
    border-radius: 50%;
    flex: none;
  }
  .avatar {
    display: grid;
    place-items: center;
    background: var(--raised-3);
    color: var(--muted);
  }
  .who {
    display: grid;
    line-height: 1.25;
  }
  .who small {
    color: var(--faint);
    font-weight: 500;
  }
  .gallery {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
    gap: 10px;
  }
  .shot {
    position: relative;
    padding: 0;
    border: 0;
    border-radius: 10px;
    overflow: hidden;
    background: var(--raised-2);
    aspect-ratio: 16 / 10;
    cursor: pointer;
  }
  .shot img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .shot span {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    padding: 16px 10px 6px;
    background: linear-gradient(transparent, rgb(0 0 0 / 0.75));
    color: #fff;
    font-size: 12px;
    text-align: left;
  }
  .full {
    width: 100%;
    max-height: calc(100vh - 220px);
    object-fit: contain;
    border-radius: 10px;
  }
  .only {
    margin-bottom: 12px;
  }
  .versions {
    display: grid;
  }
  .version {
    display: grid;
    grid-template-columns: 28px minmax(0, 1.3fr) minmax(0, 1fr) 90px auto;
    align-items: center;
    gap: 12px;
    padding: 10px 6px;
  }
  .version + .version {
    border-top: 1px solid var(--line);
  }
  .version.off > :not(:last-child) {
    opacity: 0.45;
  }
  .vtype {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border-radius: 8px;
    font-size: 12px;
    font-weight: 800;
  }
  .vtype.release {
    background: var(--ok-soft);
    color: var(--ok);
  }
  .vtype.beta {
    background: rgb(233 196 106 / 0.15);
    color: var(--warn);
  }
  .vtype.alpha {
    background: var(--danger-soft);
    color: var(--danger);
  }
  .vinfo,
  .vmeta,
  .vstats {
    display: grid;
    min-width: 0;
    line-height: 1.3;
  }
  .vinfo b,
  .vinfo small,
  .vmeta span,
  .vmeta small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .vinfo small,
  .vmeta small,
  .vstats small {
    color: var(--faint);
    font-size: 12px;
  }
  .vmeta span,
  .vstats span {
    font-size: 13px;
    color: var(--muted);
  }
  .empty {
    padding: 20px;
    text-align: center;
    color: var(--faint);
  }
</style>
