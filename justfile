fmt:
    cargo fmt --all -- --check

clippy:
    cargo clippy --all-targets --all-features -- -D warnings

test:
    cargo test --all

textlint:
    textlint $(git ls-files '*.md' | grep -v '^\.codex/')

docs_links:
    cd docs && PUPPETEER_SKIP_DOWNLOAD=1 PUPPETEER_SKIP_CHROMIUM_DOWNLOAD=1 npx --yes mint@4.2.269 broken-links

docs_vale:
    cd docs && vale sync --config .vale.ini
    cd docs && vale --config .vale.ini --glob='*.mdx' .

ci: fmt clippy test textlint docs_vale docs_links
