# LLM Spider

`llm-spider` is a Rust CLI and library.
It finds pages via the OpenAI API (web search).
It crawls under strict budgets.
It prints Markdown with sources and `TrustTier`.

## Quick start

```sh
direnv allow
just ci
```

Without direnv:

```sh
nix develop -c just ci
```

## Run (CLI)

```sh
export OPENAI_API_KEY=...
cargo run -- spider --query "example query"
```

For debug logs, set `RUST_LOG=debug`.

## Configuration

`OPENAI_API_KEY` is required.

Optional environment variables:

- `OPENAI_BASE_URL` (default: `https://api.openai.com/v1/`)
- `LLM_SPIDER_OPENAI_SEARCH_MODEL` (default: `gpt-5.2`)
- `LLM_SPIDER_OPENAI_SELECT_MODEL` (default: `gpt-5.2`)
- `LLM_SPIDER_OPENAI_REASONING_EFFORT` (default: `medium`)

For all CLI flags, run:

```sh
cargo run -- spider --help
```

## Docs

Docs live under `docs/` (Mintlify).

- Start here: `docs/index.mdx`
- Rust usage: `docs/spider/rust-usage.mdx`

## License

This repository does not include a license file yet.
Add one before expecting third-party reuse.
