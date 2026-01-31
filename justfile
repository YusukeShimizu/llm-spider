fmt:
    cargo fmt --all -- --check

proto_fmt:
    buf format -d --exit-code

proto_lint:
    #!/usr/bin/env sh
    set -eu
    buf lint
    tmpdir=$(mktemp -d)
    trap 'rm -rf "$tmpdir"' EXIT
    buf build -o "$tmpdir/descriptor.pb"
    (cd proto && api-linter -I . --descriptor-set-in "$tmpdir/descriptor.pb" --set-exit-status $(find . -name "*.proto" -print))

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

ci: fmt proto_fmt proto_lint clippy test textlint docs_vale docs_links
