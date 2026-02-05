# LLM Spider

`llm-spider` is a Rust CLI.
It finds pages via OpenAI web search.
It crawls under strict budgets.
It prints Markdown with sources.

## Quick start

```sh
direnv allow
just ci
```

Without direnv:

```sh
nix develop -c just ci
```

## Run

```sh
export OPENAI_API_KEY=...
cargo run -- spider --query "example query"
```

For debug logs, set `RUST_LOG=debug`.
