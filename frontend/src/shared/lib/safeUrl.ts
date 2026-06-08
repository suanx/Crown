/**
 * URL safety for rendering untrusted Markdown links.
 *
 * Assistant output and web_search / web_fetch tool results are UNTRUSTED:
 * a `[text](javascript:...)` or `[text](data:text/html,...)` link rendered
 * with a raw `href` runs script in the Tauri webview renderer (which can
 * reach exposed IPC). Only a small allow-list of protocols is ever safe to
 * put in an `href`; everything else degrades to a non-navigable link.
 */

/** Protocols allowed in a rendered Markdown link `href`. */
const ALLOWED_PROTOCOLS: ReadonlySet<string> = new Set([
  "http:",
  "https:",
  "mailto:",
]);

/**
 * Sanitize a Markdown link URL for use as an `href`.
 *
 * Returns the original URL when it is safe (http/https/mailto, or a
 * relative/anchor link with no dangerous scheme), or `null` when it must be
 * blocked (e.g. `javascript:`, `data:`, `vbscript:`, `file:`). Callers should
 * render a blocked link as plain, non-navigable text.
 *
 * Parsing strategy: a URL has a "scheme" only if it matches `scheme:` at the
 * very start (RFC 3986 — letters/digits/`+`/`-`/`.`, starting with a letter).
 * Relative URLs, fragments (`#x`) and query-only URLs have no scheme and are
 * treated as safe (they cannot navigate to a script context on their own).
 */
export function sanitizeLinkUrl(url: string): string | null {
  const trimmed = url.trim();
  if (trimmed === "") return null;

  // Control characters (incl. \t \n \r) are used to smuggle schemes like
  // "java\nscript:alert(1)" past naive checks — strip them before inspecting.
  // eslint-disable-next-line no-control-regex
  const cleaned = trimmed.replace(/[\u0000-\u001F\u007F]/g, "");

  const schemeMatch = /^([a-zA-Z][a-zA-Z0-9+.-]*):/.exec(cleaned);
  if (schemeMatch) {
    const scheme = `${schemeMatch[1].toLowerCase()}:`;
    return ALLOWED_PROTOCOLS.has(scheme) ? trimmed : null;
  }

  // No scheme → relative path, anchor, or query. Safe to render as href.
  return trimmed;
}
