# ExecPlan: LLM Spider MVP（OpenAI Search + 制約付きクロール）

## Goal

自然言語クエリから「信頼できる候補」を探索する。
制約（ページ数/深さ/時間/文字数）を守って、最小限のページを収集する。
出典 URL と `TrustTier` を明示した Markdown を生成できるようにする。

### 非目的

- Web の網羅的クロール/アーカイブは行わない。
- ToS 逸脱（ペイウォール回避等）は行わない。
- JavaScript 実行やブラウザ自動化は行わない（HTML は不正入力として扱う）。

## Scope

### 変更対象（in-scope）

- CLI に `spider`（仮）サブコマンドを追加し、`UserRequest`（予算含む）を受け付ける。
- OpenAI Search（自然言語クエリ）で初期 URL を収集できるようにする。
- LLM による子ページ（リンク）選定機能を追加する。
- 収集制約: `max_pages` / `max_depth` / `max_elapsed` / host 単位レート制限 / タイムアウト / リトライ上限。
- `OutputComposer` による Markdown 出力（`max_chars` を超えない、本文と出典対応、`TrustTier` 明示）。
- ログは `tracing` を使用し、`RUST_LOG` で詳細度を制御する。
- Integration Test（`tests/`）で代表操作を検証する（必要に応じて mock を使う）。

### 変更しない（out-of-scope）

- 関連度の連続スコア最適化や学習、設定ファイルでの分類ルール変更はしない（MVP はハードコード）。
- robots.txt の完全準拠やサイトごとの高度なクローリングポリシーは後回しにする（最低限の負荷軽減を優先）。
- OpenAI のモデル学習や Fine-tuning は行わない。

## Milestones

### M0: 命名と仕様の同期（テンプレート→プロジェクト）

#### 観測可能な成果

- `Cargo.toml` の `package.name` をプロジェクト名に揃え、`cargo test --all` が通る。
- `README.md` の実行例が `spider`（仮）を含む形に更新される。
- `spec.md` に「LLM Spider MVP」の Concept（最低限: CLI/収集制約/出典付き Markdown）を追記し、`just textlint` が通る。

#### 補足

- 既存の `hello` サブコマンドは残してよい（後方互換とテンプレ機能の保持）。不要なら M1 以降で削除判断する。

### M1: `UserRequest` と入力（予算）

#### 観測可能な成果

- `llm-spider spider --query "<text>"` で `UserRequest` が構築される。
  debug ログに構造化出力される。
- CLI から予算を指定できる。
  指定できるフラグは次のとおりである。
  ```
  --max-chars
  --min-sources
  --search-limit
  --max-pages
  --max-depth
  --max-elapsed
  --max-child-candidates
  --max-children-per-page
  ```
  デフォルトは次のとおりである。
  ```
  max_chars = 4000
  min_sources = 3
  search_limit = 10
  max_pages = 20
  max_depth = 1
  max_elapsed = 30s
  max_child_candidates = 20
  max_children_per_page = 3
  ```

### M2: Trust 分類（URL / ドメイン）

#### 観測可能な成果

- `TrustTier::{High,Medium,Low}` と判定規則（例: `*.gov`, `*.edu`, `*.ac.*`, `*.go.jp` など）を実装し、URL/ドメインから決定できる。
- フォールバックが発生した場合、最終出力の出典一覧に `TrustTier` が必ず表示される。

### M3: OpenAI Search によるページ特定（初期 URL 収集）

#### 観測可能な成果

- `OpenAiSearchProvider` を実装する。
  自然言語クエリから `search_limit` 件の URL を返す。
- `OPENAI_API_KEY` が未設定の場合、わかりやすいエラーで終了する。

### M4: 制約付きクロール（Frontier + Fetcher + Extractor）

#### 観測可能な成果

- `max_pages/max_depth/max_elapsed` が厳密に守られる（超えない）。
- Fetch は安全側に倒す（`http/https` のみ、危険スキーム拒否、タイムアウト、ホスト単位のレート制限、リトライ上限）。
- Extractor が HTML から本文テキストとリンクを抽出する。

#### 実装メモ（拘束力は低い）

- 初期は単純な BFS/優先度付きキューで良い。優先度は `(TrustTier, search順位)` で trust を優先する。
- 依存クレートは必要最小限に抑える（後から置換できる境界を作る）。

### M5: LLM による子ページ特定（リンク選定）

#### 観測可能な成果

- `ChildLinkSelector` を実装する。
  ページのリンク候補から、`max_children_per_page` 件を選ぶ。
- LLM に入力するのは「抽出済みテキスト（抜粋）」と「リンク候補（URL とアンカーテキスト）」のみである。
  ページ本文の指示による方針変更は許可しない。
- コスト抑制のために、LLM に渡す候補数を `max_child_candidates` に制限する。

### M6: Markdown 出力（`max_chars` 内、出典対応）

#### 観測可能な成果

- 出力は Markdown のみで、本文の各主張（箇条書き等）と出典 URL が対応する。
- `max_chars` を必ず守り、超える場合は要約/削減の方針が一貫している（例: 低 `TrustTier` の内容から削る）。
- `min_sources` を満たせない場合でも、満たせなかったことと理由（予算切れ/スコープ制約等）を明示する。

## Tests

### 原則

- Integration Test を優先し、外部ネットワークへ依存しない。
  必要に応じて mock を使う。
- 外部ネットワークには依存しない（テスト内でローカル HTTP サーバを立て、固定の HTML/リンク構造を提供する）。

### 具体案

- `tests/spider_basic.rs` で次を検証する。
  - テスト内で `127.0.0.1:<port>` にローカル HTTP サーバを起動し、次を提供する:
    - `/start` → 2〜3 個の内部リンク（`/a`, `/b`）と本文を含む HTML
    - `/a`, `/b` → 本文と出典候補
  - `--max-pages=1` で 1 ページしか取得しないことを検証する。
  - `--max-depth=0/1` でリンク展開の有無を検証する。
  - `--max-elapsed=1ms` 等でタイムアウト/時間制約の動作を検証する（flaky 回避のため余裕を持った設計にする）。
  - 出力が Markdown であり、出典 URL が含まれ、`TrustTier` 表示があることを検証する。
- 既存の `tests/cli_hello.rs` は M0 のリネームに追従する（バイナリ名の更新）。
- OpenAI API はテスト内で mock する。
  `OPENAI_BASE_URL` をローカル HTTP サーバへ向ける。

### 品質ゲート

- `just ci`（fmt/proto/clippy/test/textlint/docs）を通す。
- Markdown を更新したら `just textlint` を先に通す。

## Decisions / Risks

### Decisions（重要判断）

- MVP は「信頼性（trust）優先」を固定し、関連性（relevance）は最低限（search 順序）から開始する。
- Trust は連続スコアにせず `TrustTier` の離散のみとし、判定規則はハードコードする。
- OpenAI Search を必須の入口とする。
  CI と Integration Test は外部ネットワークへ依存しない。
- 関連性の特定は LLM を導入し、子ページ（リンク）選定に使う。
  trust は relevance より優先する。
- LLM 呼び出しは、リンク候補が `max_children_per_page` を超える場合に限定する。
- HTML は不正入力として扱い、安全なサブセットのみ処理する（JS 実行なし、危険 URL スキーム拒否）。

### Risks（リスクと緩和）

- 法務/ToS: 取得対象の規約違反のリスク → User-Agent、負荷軽減、明示的な非目的（ペイウォール回避しない）を維持する。
- 安全性: 悪意ある HTML/URL による SSRF/DoS → `http/https` 限定、IP/ローカルアドレス拒否（必要なら）、サイズ制限、タイムアウト、リトライ上限。
- テストの不安定性: 時間制約テストが flaky → 時間依存を最小化し、可能なら「経過時間」ではなく「予算到達」で検証する。
- 依存増加: spider crate 等の採用が重くなる → 境界（Fetcher/Extractor/Frontier）を作り、後から入替可能にする。
- コスト/プライバシー: OpenAI API の送信が発生する → 送信対象を最小化し、設定と注意事項を明示する。
- プロンプトインジェクション: ページが LLM の方針を誘導する → 入力制限と方針固定で緩和する。

## Progress

- 2026-02-04: `draft.md` を元に ExecPlan を作成した（未実装）。
- 2026-02-04: 作業用 worktree（`feat-llm-spider-mvp`）を作成し、M0 のリネーム（`llm-spider`）に着手した。
- 2026-02-04: `spider` サブコマンドを追加し、OpenAI Search で初期ページを特定できるようにした。
- 2026-02-04: LLM による子ページ（リンク）選定を追加し、制約付きで収集できるようにした。
- 2026-02-04: Integration Test で OpenAI API を mock した。
  外部ネットワーク無しで `max_pages` / `max_depth` / `max_chars` とリンク選定を検証した。
- 2026-02-04: `just ci` が通ることを確認した。
