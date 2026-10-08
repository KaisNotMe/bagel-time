// Turning Modrinth's Markdown and SVG into safe HTML.
import DOMPurify from "dompurify";
import { marked } from "marked";

const EMBEDS = ["https://www.youtube.com/embed/", "https://www.youtube-nocookie.com/embed/"];

// Project pages often embed YouTube videos; any other iframe is dropped.
DOMPurify.addHook("uponSanitizeElement", (node, data) => {
  if (data.tagName === "iframe") {
    const src = (node as Element).getAttribute?.("src") ?? "";
    if (!EMBEDS.some((p) => src.startsWith(p))) node.parentNode?.removeChild(node);
  }
});

export function renderMarkdown(source: string): string {
  const html = marked.parse(source, { async: false, gfm: true });
  return DOMPurify.sanitize(html, {
    ADD_TAGS: ["iframe"],
    ADD_ATTR: ["allowfullscreen", "frameborder"],
  });
}

/** Category icons from the Modrinth API are inline SVG markup. */
export function safeSvg(svg: string): string {
  return DOMPurify.sanitize(svg, { USE_PROFILES: { svg: true } });
}
