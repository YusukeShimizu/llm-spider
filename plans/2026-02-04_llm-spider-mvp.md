# ExecPlan: LLM Spider MVP

## Goal

- Find candidate pages from a natural language query via OpenAI web search.
- Crawl under strict budgets (pages/depth/time/chars) and respect `robots.txt`.
- Output Markdown with source URLs and `TrustTier`.

## Non-goals

- Exhaustive crawling or archiving.
- Bypassing ToS/paywalls.
- Running JavaScript or browser automation.

## Scope

- CLI: `llm-spider spider --query "<text>"`.
- LLM-assisted child-link selection.
- Structured logs via `tracing` (`RUST_LOG`).
- Integration tests without external network access.
- Quality gates via `just ci`.

## Status

- Implemented and passing `just ci` (as of 2026-02-04).
