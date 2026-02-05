# LLM Spider Specification

This repository provides `llm-spider`.
It is a CLI that finds pages via OpenAI web search.
It crawls under strict budgets and outputs Markdown with sources.

This document (`spec.md`) defines required behavior and non-negotiable development constraints.

This spec follows arXiv:2508.14511v2 ("What You See Is What It Does").
It is structured as Concept Specifications and Synchronizations.
Reference: https://arxiv.org/html/2508.14511v2

## Concepts

```text
concept Shell
purpose
    Represent command execution from outside (developers/CI).
state
    env: string -> string
actions
    request [ command: string ]
        => [ ]
```

```text
concept DevEnvironment
purpose
    Provide a reproducible dev environment via Nix Flakes.
    Manage env vars via direnv (`.envrc`).
    Make Rust stdlib sources discoverable for rust-analyzer.
state
    flake_nix: string
    flake_lock: string
    envrc: string
    envrc_local: string
    rust_src_path: string
    rust_lib_src_link: string
actions
    enter [ tool: "direnv" ]
        => [ ]
        load the dev shell via `use flake` in `.envrc`
        load local-only overrides from `.envrc.local` when present
        export `RUST_SRC_PATH` for Rust stdlib sources
        ensure `rust-lib-src` points to the stdlib sources
    enter [ tool: "nix" ]
        => [ ]
        enter the dev shell via `nix develop`
        export `RUST_SRC_PATH` for Rust stdlib sources
        ensure `rust-lib-src` points to the stdlib sources
operational principle
    after enter [ tool: "direnv" ]
        => [ ]
    then Shell/request [ command: "just ci" ]
        => [ ]
```

```text
concept SpiderCLI
purpose
    Find pages from a natural language query via OpenAI web search,
    crawl under budgets, and produce Markdown with sources.
state
    openai_api_key: string
    reasoning_effort: string
    search_limit: int
    max_chars: int
    min_sources: int
    max_pages: int
    max_depth: int
    max_elapsed: string
    max_child_candidates: int
    max_children_per_page: int
actions
    crawl [ query: string ]
        => [ exit_code: 0 ; stdout: "Markdown\n" ]
        stdout is Markdown.
        stdout includes source URLs and a `TrustTier` per source.
        `TrustTier` is assigned by the LLM (not hard-coded domain rules).
        Reasoning effort is configurable via `reasoning_effort` (default: `medium`).
        Crawling respects `robots.txt`.
        Collection is limited by `max_pages` / `max_depth` / `max_elapsed`.
operational principle
    after crawl [ query: "example query" ]
        => [ exit_code: 0 ]
```

```text
concept Logging
purpose
    Provide structured logs via tracing.
    Control log verbosity via `RUST_LOG`.
state
    rust_log: string
actions
    init [ ]
        => [ ]
        configure `tracing_subscriber::EnvFilter` from `RUST_LOG`
        default to `info` when `RUST_LOG` is not set
        write logs to stderr
        when the effective filter enables debug, emit debug logs (e.g. `"parsed cli"`)
operational principle
    after init [ ]
        => [ ]
    then SpiderCLI/crawl [ query: "example query" ]
        => [ exit_code: 0 ]
```

```text
concept RustToolchain
purpose
    Provide standard quality gates for Rust.
state
    src_dir: string
    tests_dir: string
actions
    fmt_check [ ]
        => [ ok: boolean ]
        run `cargo fmt --all -- --check`
    clippy [ ]
        => [ ok: boolean ]
        run `cargo clippy --all-targets --all-features -- -D warnings`
    test [ ]
        => [ ok: boolean ]
        run `cargo test --all`
```

```text
concept IntegrationTests
purpose
    Express representative behavior as integration tests (`tests/`).
    Tests must not depend on the external network.
state
    tests_dir: string
actions
    run [ ]
        => [ ok: boolean ]
        run `cargo test --all`
```

```text
concept Documentation
purpose
    Keep docs under `docs/` and check style/links.
state
    docs_dir: string
    docs_json: string
    vale_ini: string
actions
    vale [ ]
        => [ ok: boolean ]
        run `vale --config docs/.vale.ini --glob='*.mdx' docs`
    broken_links [ ]
        => [ ok: boolean ]
        run `mint broken-links` for `docs/`
```

```text
concept Textlint
purpose
    Lint Markdown text with textlint.
state
    config: string
    prh: string
actions
    lint_markdown [ ]
        => [ ok: boolean ]
        run `textlint` for tracked `*.md` files (excluding `.codex/`)
```

## Synchronizations

```text
sync CI
when {
    Shell/request: [ command: "just ci" ]
        => [] }
then {
    RustToolchain/fmt_check: [ ]
    RustToolchain/clippy: [ ]
    RustToolchain/test: [ ]
    Textlint/lint_markdown: [ ]
    Documentation/vale: [ ]
    Documentation/broken_links: [ ] }
```
