import { Marked } from "marked";

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

const renderer = new Marked({
  gfm: true,
  breaks: true,
  renderer: {
    html({ text }) {
      return escapeHtml(text);
    },
    link({ href, title, text }) {
      const safeHref =
        href && /^(https?:|mailto:|#)/i.test(href.trim()) ? escapeHtml(href) : "#";
      const titleAttr = title ? ` title="${escapeHtml(title)}"` : "";
      return `<a href="${safeHref}" target="_blank" rel="noopener noreferrer"${titleAttr}>${text}</a>`;
    },
  },
});

export function renderMarkdown(source: string): string {
  if (!source) return "";
  try {
    return renderer.parse(source) as string;
  } catch {
    return `<pre>${escapeHtml(source)}</pre>`;
  }
}
