/**
 * 网搜结果体 — 把后端 web_search 的纯文本结果解析成卡片式展示。
 *
 * 后端格式（crates/tools/src/web/mod.rs）：
 *   Search results for: {query}
 *
 *   1. {title}
 *      URL: {url}
 *      {snippet}
 */

import { useState } from "react";
import { ResultPre } from "./toolCardBodies";
import { sanitizeLinkUrl } from "@/shared/lib/safeUrl";

interface SearchHit {
  title: string;
  url: string;
  domain: string;
  snippet: string;
  isImage: boolean;
}

const IMAGE_EXTS = /\.(png|jpg|jpeg|gif|webp|bmp|svg)(\?|$)/i;

function isImageUrl(url: string): boolean {
  return IMAGE_EXTS.test(url.split("?")[0].split("#")[0]);
}

/** 解析后端 web_search 结果文本为结构化命中列表。 */
export function parseWebSearchResults(result: string): SearchHit[] {
  const hits: SearchHit[] = [];
  const lines = result.split(/\r?\n/);
  let title = "";
  let url = "";
  for (const line of lines) {
    const titleMatch = line.match(/^\s*\d+\.\s+(.*)$/);
    if (titleMatch) { title = titleMatch[1].trim(); continue; }
    const urlMatch = line.match(/^\s*URL:\s*(.*)$/i);
    if (urlMatch) { url = urlMatch[1].trim(); continue; }
    if (url && line.trim()) {
      let domain = url;
      try { domain = new URL(url).hostname.replace(/^www\./, ""); } catch {}
      hits.push({ title: title || domain, url, domain, snippet: line.trim(), isImage: isImageUrl(url) });
      title = "";
      url = "";
    }
  }
  // Flush pending if no snippet followed URL
  if (url && !hits.length) {
    let domain = url;
    try { domain = new URL(url).hostname.replace(/^www\./, ""); } catch {}
    hits.push({ title: title || domain, url, domain, snippet: "", isImage: isImageUrl(url) });
  }
  return hits;
}

export function WebSearchBody({ result }: { result: string }) {
  const hits = parseWebSearchResults(result);
  if (hits.length === 0) return <ResultPre text={result} collapsible />;
  return (
    <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
      {hits.map((hit, i) => (
        <SearchCard key={`${hit.url}-${i}`} hit={hit} />
      ))}
    </div>
  );
}

function SearchCard({ hit }: { hit: SearchHit }) {
  const safeHref = sanitizeLinkUrl(hit.url);
  const [imgFailed, setImgFailed] = useState(false);

  const card = (
    <div className="rounded-lg border border-border-subtle bg-elevated p-3 hover:border-border-default transition-colors h-full flex flex-col">
      <div className="flex items-start gap-2.5 min-w-0">
        {hit.isImage ? (
          <div className="shrink-0 w-10 h-10 rounded bg-canvas border border-border-subtle flex items-center justify-center text-[18px] overflow-hidden">
            {!imgFailed ? <img src={hit.url} alt="" className="w-full h-full object-cover" onError={() => setImgFailed(true)} loading="lazy" /> : "🖼️"}
          </div>
        ) : (
          <Favicon domain={hit.domain} />
        )}
        <div className="flex-1 min-w-0">
          <div className="text-sm font-medium text-text-primary truncate">{hit.title}</div>
          {hit.snippet && <div className="text-xs text-text-tertiary mt-1 line-clamp-2 leading-relaxed">{hit.snippet}</div>}
          <div className="text-[10px] text-text-tertiary mt-1.5 font-mono truncate">{hit.domain}</div>
        </div>
      </div>
    </div>
  );

  if (safeHref === null) return card;
  return <a href={safeHref} target="_blank" rel="noopener noreferrer" className="block focus-ring rounded-lg">{card}</a>;
}

function Favicon({ domain }: { domain: string }) {
  const [failed, setFailed] = useState(false);
  if (failed || !domain) return <div className="shrink-0 w-8 h-8 rounded bg-text-tertiary/10" />;
  return (
    <img src={`https://www.google.com/s2/favicons?domain=${encodeURIComponent(domain)}&sz=32`}
      alt="" width={16} height={16} className="shrink-0 w-8 h-8 rounded object-contain bg-canvas border border-border-subtle p-1"
      onError={() => setFailed(true)} loading="lazy" />
  );
}
