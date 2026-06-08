/**
 * 网搜结果体 — 把后端 web_search 的纯文本结果解析成「favicon + 标题 + 域名」
 * 列表（对齐 Claude 桌面端 `Searched the web` 展开样式）。
 *
 * 后端格式（crates/tools/src/web/mod.rs）：
 *   Search results for: {query}
 *
 *   1. {title}
 *      URL: {url}
 *      {snippet}
 *
 *   2. {title}
 *      ...
 *
 * 解析失败时回退到纯文本（ResultPre）。favicon 用 Google s2 服务按域名取，
 * 取不到时静默隐藏（onError），不显破图。
 */

import { useState } from "react";
import { ResultPre } from "./toolCardBodies";
import { sanitizeLinkUrl } from "@/shared/lib/safeUrl";

interface SearchHit {
  title: string;
  url: string;
  domain: string;
}

/** 解析后端 web_search 结果文本为结构化命中列表。 */
export function parseWebSearchResults(result: string): SearchHit[] {
  const hits: SearchHit[] = [];
  const lines = result.split(/\r?\n/);
  let title = "";
  for (const line of lines) {
    const titleMatch = line.match(/^\s*\d+\.\s+(.*)$/);
    if (titleMatch) {
      title = titleMatch[1].trim();
      continue;
    }
    const urlMatch = line.match(/^\s*URL:\s*(.*)$/i);
    if (urlMatch) {
      const url = urlMatch[1].trim();
      let domain = url;
      try {
        domain = new URL(url).hostname.replace(/^www\./, "");
      } catch {
        // 非法 URL：保留原串作域名展示。
      }
      hits.push({ title: title || domain, url, domain });
      title = "";
    }
  }
  return hits;
}

export function WebSearchBody({ result }: { result: string }) {
  const hits = parseWebSearchResults(result);
  if (hits.length === 0) {
    // 解析不出结构化命中 → 回退纯文本，绝不显空。
    return <ResultPre text={result} collapsible />;
  }
  return (
    <div className="space-y-0.5">
      {hits.map((hit, i) => (
        <SearchHitRow key={`${hit.url}-${i}`} hit={hit} />
      ))}
    </div>
  );
}

function SearchHitRow({ hit }: { hit: SearchHit }) {
  const safeHref = sanitizeLinkUrl(hit.url);
  const Row = (
    <div className="flex items-center gap-2.5 py-1 min-w-0">
      <Favicon domain={hit.domain} />
      <span className="text-sm text-text-secondary truncate min-w-0 flex-1">
        {hit.title}
      </span>
      <span className="text-xs text-text-tertiary shrink-0 font-mono">
        {hit.domain}
      </span>
    </div>
  );
  if (safeHref === null) return Row;
  return (
    <a
      href={safeHref}
      target="_blank"
      rel="noopener noreferrer"
      className="block rounded-md -mx-1.5 px-1.5 hover:bg-hover transition-colors focus-ring"
    >
      {Row}
    </a>
  );
}

function Favicon({ domain }: { domain: string }) {
  const [failed, setFailed] = useState(false);
  if (failed || !domain) {
    return <span className="shrink-0 h-4 w-4 rounded-sm bg-text-tertiary/20" />;
  }
  return (
    <img
      src={`https://www.google.com/s2/favicons?domain=${encodeURIComponent(domain)}&sz=32`}
      alt=""
      width={16}
      height={16}
      className="shrink-0 h-4 w-4 rounded-sm"
      onError={() => setFailed(true)}
      loading="lazy"
    />
  );
}
