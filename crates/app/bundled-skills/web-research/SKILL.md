---
name: web-research
description: 获取最新资讯、文档、新闻、事实核查，适用于话题调研、产品对比、问题排查等场景。
metadata:
  display-name: 联网搜索
  icon: "\U0001F310"
  usage: "/web-research <research topic>"
---

# Web Research

Search the web for current information and generate structured research summaries. This skill orchestrates multiple search queries, deep-reads key sources, cross-references findings, and produces a reliable, cited report.

## When to Use

- **Current information** — Events, news, or data after knowledge cutoff
- **Latest documentation** — Up-to-date framework/library docs, API changes
- **Real-time data** — Prices, status, scores, statistics, stock data
- **Fact verification** — Check current status of projects, companies, technologies
- **Recent discussions** — Community opinions, GitHub issues, Stack Overflow answers
- **Product comparisons** — Latest reviews, benchmarks, and comparisons
- **Troubleshooting** — Search for specific error messages or solutions
- **Competitive analysis** — Company news, product launches, market positioning
- **Regulatory/policy** — Latest regulations, compliance requirements

## Workflow

### Phase 1: Query Planning
Before searching, decompose the topic into 3-5 search angles:

1. **Core query** — The most direct search for the topic
2. **Context query** — Background, history, or related concepts
3. **Comparison query** — Alternatives, competitors, or trade-offs
4. **Recency query** — Latest news, updates, or changes (add year/date)
5. **Expert query** — Technical deep-dive, academic, or authoritative sources

### Phase 2: Search Execution
使用搜索工具执行每个规划的查询。搜索工具由外部服务提供：
- 查看可用的搜索服务
- 调用搜索工具执行查询
- 若无搜索服务可用，使用可用的网页抓取工具获取信息

Assess results before deep reading.

### Phase 3: Deep Reading
抓取 2-4 个最相关、最权威的搜索结果的完整内容。优先选择：
- Official documentation and primary sources
- Recent articles (< 6 months old)
- Sources with concrete data, benchmarks, or code examples
- Multiple independent sources for cross-verification

### Phase 4: Synthesis
- Cross-reference findings across multiple sources
- Identify consensus vs. disagreement
- Note information gaps or unanswered questions
- Distinguish facts from opinions

### Phase 5: Report Generation
Write a structured research summary (see Output Format below).

## Search Strategy

### Query Construction Patterns

| Research Type | Query Pattern | Example |
|-------------|--------------|---------|
| Technology overview | `"[tech] [year] features guide"` | `"React 19 2025 features guide"` |
| Troubleshooting | `"[exact error message]" [framework]` | `"Cannot read property 'map' of undefined" React` |
| Comparison | `"[A] vs [B] [year] comparison"` | `"Next.js vs Nuxt 2025 comparison"` |
| Best practices | `"[topic] best practices [year]"` | `"TypeScript monorepo best practices 2025"` |
| Migration | `"[tech] migration guide [version]"` | `"Angular migration guide v17 to v18"` |
| Security | `"[tech] security vulnerability CVE [year]"` | `"Log4j security vulnerability CVE 2024"` |
| Performance | `"[tech] benchmark performance [year]"` | `"Bun vs Node.js benchmark performance 2025"` |
| Integration | `"[tech A] [tech B] integration tutorial"` | `"Stripe Next.js integration tutorial"` |

### Multi-Angle Search Example

```
Topic: "Is Bun ready for production use in 2025?"

Query 1 (Core):       "Bun production ready 2025"
Query 2 (Experience): "Bun production experience issues site:reddit.com OR site:news.ycombinator.com"
Query 3 (Comparison): "Bun vs Node.js production comparison 2025"
Query 4 (Technical):  "Bun compatibility Node.js packages ecosystem"
Query 5 (Recent):     "Bun 1.2 release changelog features"
```

### Search Tips

- **Exact phrases**: Use quotes for exact error messages or specific terms
- **Site-specific**: Add `site:github.com`, `site:stackoverflow.com` for targeted results
- **Recency**: Always include the current year for technology topics
- **Exclusions**: Use `-` to exclude irrelevant results (e.g., `-tutorial` when you want docs)
- **Broadening**: If initial results are thin, remove specific version numbers or dates
- **Language**: For Chinese tech community insights, also search in Chinese: `"Bun 生产环境 2025"`

## Source Credibility Assessment

When evaluating sources, consider reliability:

| Tier | Source Type | Examples | Trust Level |
|------|-----------|---------|-------------|
| T1 | Official documentation, RFCs, specs | MDN, official docs, GitHub repos | Highest |
| T2 | Established tech publications | InfoQ, The Verge, Ars Technica | High |
| T3 | Developer blogs from known experts | Martin Fowler, Dan Abramov, etc. | High |
| T4 | Community discussions | Stack Overflow, GitHub Discussions | Medium (verify) |
| T5 | Social media, forums | Reddit, HN, Twitter/X | Medium (opinions) |
| T6 | AI-generated content, SEO farms | Generic blogs, content farms | Low (cross-check) |

**Cross-reference rule**: Any claim should be verified by at least 2 independent sources before stating as fact. If only one source, note it explicitly.

## Handling Time-Sensitive Information

- **Breaking news**: Prioritize last 24-48 hours; note that info may be incomplete
- **Rapidly evolving topics**: Check official changelogs and release notes first
- **Historical context**: Search for original announcements and decision rationale
- **Deprecated information**: Always verify the date of articles; explicitly warn about outdated info
- **Version-specific**: Always match documentation version to the user's actual version

## Research Patterns

### Technology Evaluation
1. Search official docs + GitHub for capabilities
2. Search for production usage stories (Reddit, HN, blog posts)
3. Search for known issues and limitations
4. Search for alternatives and comparisons
5. Synthesize: capability, maturity, community, trade-offs

### Troubleshooting
1. Search exact error message (in quotes)
2. Search error + framework + version
3. Check GitHub Issues for the relevant repo
4. Check Stack Overflow for similar problems
5. Synthesize: root cause, solutions, workarounds

### Competitive Analysis
1. Search each competitor's official site for features
2. Search for head-to-head comparison articles
3. Search for user reviews and satisfaction
4. Search for pricing and business model changes
5. Synthesize: feature matrix, pricing, strengths, weaknesses

## Output Format

Save research summary as `research-summary.md`:

```markdown
# Research Summary: [Topic]

**Date**: [YYYY-MM-DD]
**Query count**: N searches performed
**Sources consulted**: N

## Overview
[1-2 paragraph executive summary of findings]

## Key Findings
1. **[Finding title]**: [Description + supporting evidence + source]
2. **[Finding title]**: [Description + supporting evidence + source]
3. **[Finding title]**: [Description + supporting evidence + source]

## Detailed Analysis

### [Subtopic 1]
[In-depth analysis with citations]

### [Subtopic 2]
[In-depth analysis with citations]

## Consensus vs. Disagreement
- **Widely agreed**: [Points most sources agree on]
- **Debated**: [Points where sources disagree + both sides]
- **Uncertain**: [Areas where information is insufficient]

## Sources
| # | Source | Type | Date | Key Takeaway |
|---|--------|------|------|-------------|
| 1 | [Title](URL) | Official docs | YYYY-MM | [Takeaway] |
| 2 | [Title](URL) | Blog post | YYYY-MM | [Takeaway] |
| 3 | [Title](URL) | Discussion | YYYY-MM | [Takeaway] |

## Information Gaps
- [Questions that couldn't be answered]
- [Topics that need deeper investigation]

## Further Research
- [Suggested follow-up topics or searches]
```

## Guidelines

- Write reports in Chinese
- Cite ALL information sources with URLs and dates
- Clearly distinguish facts from opinions
- Note the date/recency of each source
- Flag potentially outdated information
- Save report to workspace as `research-summary.md`
- If web search tools are unavailable, inform the user and suggest alternative approaches
- When findings conflict, present both sides rather than picking one
- Always include an "Information Gaps" section for intellectual honesty
