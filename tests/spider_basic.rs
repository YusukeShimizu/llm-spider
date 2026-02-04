use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use llm_spider::openai::{OpenAiApi, SearchHit};
use llm_spider::spider::{FetchedPage, PageFetcher, TrustTier, crawl_with_fetcher};
use url::Url;

#[derive(Default)]
struct FakeOpenAi {
    hits: Vec<Url>,
    selected_by_page: HashMap<String, Vec<Url>>,
    select_calls: Arc<AtomicUsize>,
}

impl FakeOpenAi {
    fn with_hits(mut self, hits: Vec<&str>) -> Self {
        self.hits = hits.into_iter().map(|u| Url::parse(u).unwrap()).collect();
        self
    }

    fn with_selected(mut self, page_url: &str, selected: Vec<&str>) -> Self {
        self.selected_by_page.insert(
            page_url.to_owned(),
            selected
                .into_iter()
                .map(|u| Url::parse(u).unwrap())
                .collect(),
        );
        self
    }
}

impl OpenAiApi for FakeOpenAi {
    fn web_search(&self, _query: &str, limit: usize) -> anyhow::Result<Vec<SearchHit>> {
        Ok(self
            .hits
            .iter()
            .take(limit)
            .cloned()
            .map(|url| SearchHit { url, title: None })
            .collect())
    }

    fn select_child_links(
        &self,
        _query: &str,
        page_url: &Url,
        _page_excerpt: &str,
        _candidates: &[serde_json::Value],
        max_select: usize,
    ) -> anyhow::Result<Vec<Url>> {
        self.select_calls.fetch_add(1, Ordering::Relaxed);
        let selected = self
            .selected_by_page
            .get(page_url.as_str())
            .cloned()
            .unwrap_or_default();
        Ok(selected.into_iter().take(max_select).collect())
    }
}

#[derive(Default)]
struct FakeFetcher {
    pages: HashMap<String, FetchedPage>,
    disallow: Vec<String>,
}

impl FakeFetcher {
    fn with_page(mut self, url: &str, html: &str, links: Vec<&str>) -> Self {
        self.pages.insert(
            url.to_owned(),
            FetchedPage {
                html: html.to_owned(),
                links: links
                    .into_iter()
                    .map(|u| Url::parse(u).unwrap())
                    .collect::<Vec<_>>(),
                robots_delay: Duration::from_millis(0),
            },
        );
        self
    }

    fn with_robots_disallow(mut self, url: &str) -> Self {
        self.disallow.push(url.to_owned());
        self
    }
}

impl PageFetcher for FakeFetcher {
    fn fetch(&self, url: &Url) -> anyhow::Result<FetchedPage> {
        if self.disallow.iter().any(|u| u == url.as_str()) {
            anyhow::bail!("blocked by robots.txt");
        }
        self.pages
            .get(url.as_str())
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("missing page fixture: {url}"))
    }
}

fn request(query: &str) -> llm_spider::spider::UserRequest {
    llm_spider::spider::UserRequest {
        query: query.to_owned(),
        max_chars: 4000,
        min_sources: 1,
        search_limit: 10,
        max_pages: 20,
        max_depth: 1,
        max_elapsed: Duration::from_secs(30),
        max_child_candidates: 20,
        max_children_per_page: 3,
        allow_local: false,
    }
}

#[test]
fn spider_respects_max_pages() {
    let start = "https://example.test/start";
    let a = "https://example.test/a";
    let openai = FakeOpenAi::default()
        .with_hits(vec![start])
        .with_selected(start, vec![a]);

    let fetcher = FakeFetcher::default()
        .with_page(start, "<main>start</main>", vec![a])
        .with_page(a, "<main>a</main>", vec![]);

    let mut req = request("q");
    req.max_pages = 1;
    req.max_depth = 1;

    let result = crawl_with_fetcher(&req, &openai, &fetcher).expect("crawl");
    assert_eq!(result.sources.len(), 1);
    assert_eq!(result.sources[0].url.as_str(), start);
}

#[test]
fn spider_respects_max_depth_zero() {
    let start = "https://example.test/start";
    let a = "https://example.test/a";
    let openai = FakeOpenAi::default()
        .with_hits(vec![start])
        .with_selected(start, vec![a]);

    let fetcher = FakeFetcher::default()
        .with_page(start, "<main>start</main>", vec![a])
        .with_page(a, "<main>a</main>", vec![]);

    let mut req = request("q");
    req.max_pages = 10;
    req.max_depth = 0;

    let result = crawl_with_fetcher(&req, &openai, &fetcher).expect("crawl");
    assert_eq!(result.sources.len(), 1);
    assert!(result.sources.iter().all(|s| s.url.as_str() != a));
}

#[test]
fn spider_output_respects_max_chars() {
    let start = "https://example.test/start";
    let openai = FakeOpenAi::default().with_hits(vec![start]);
    let fetcher = FakeFetcher::default().with_page(start, "<main>hello</main>", vec![]);

    let mut req = request("q");
    req.max_pages = 1;
    req.max_depth = 0;
    req.max_chars = 80;

    let result = crawl_with_fetcher(&req, &openai, &fetcher).expect("crawl");
    let markdown = llm_spider::spider::compose_markdown(&req, &result);
    assert!(markdown.chars().count() <= 80);
}

#[test]
fn spider_llm_selects_child_links_only_when_needed() {
    let start = "https://example.test/start";
    let a = "https://example.test/a";
    let b = "https://example.test/b";

    let select_calls = Arc::new(AtomicUsize::new(0));
    let mut openai = FakeOpenAi::default()
        .with_hits(vec![start])
        .with_selected(start, vec![a]);
    openai.select_calls = Arc::clone(&select_calls);

    let fetcher = FakeFetcher::default()
        .with_page(start, "<main>start</main>", vec![a, b])
        .with_page(a, "<main>a</main>", vec![])
        .with_page(b, "<main>b</main>", vec![]);

    let mut req = request("q");
    req.max_pages = 2;
    req.max_depth = 1;
    req.max_children_per_page = 1;

    let result = crawl_with_fetcher(&req, &openai, &fetcher).expect("crawl");
    assert_eq!(result.sources.len(), 2);
    assert_eq!(select_calls.load(Ordering::Relaxed), 1);
}

#[test]
fn spider_respects_robots_txt_disallow() {
    let start = "https://example.test/start";
    let a = "https://example.test/a";
    let b = "https://example.test/b";
    let openai = FakeOpenAi::default()
        .with_hits(vec![start])
        .with_selected(start, vec![a, b]);

    let fetcher = FakeFetcher::default()
        .with_page(start, "<main>start</main>", vec![a, b])
        .with_page(b, "<main>b</main>", vec![])
        .with_robots_disallow(a);

    let mut req = request("q");
    req.max_pages = 2;
    req.max_depth = 1;

    let result = crawl_with_fetcher(&req, &openai, &fetcher).expect("crawl");
    assert!(result.sources.iter().any(|s| s.url.as_str() == b));
    assert!(result.sources.iter().all(|s| s.url.as_str() != a));

    // TrustTier is still computed and must be present in sources.
    assert!(result.sources.iter().all(|s| matches!(
        s.trust_tier,
        TrustTier::High | TrustTier::Medium | TrustTier::Low
    )));
}
