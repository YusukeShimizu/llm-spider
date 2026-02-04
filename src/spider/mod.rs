use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

use anyhow::Context as _;
use scraper::{ElementRef, Html, Selector};
use serde_json::{Value, json};
use tracing::warn;
use url::Url;

mod trust;

pub use trust::{TrustTier, classify_trust_tier};

#[derive(Debug, Clone)]
pub struct UserRequest {
    pub query: String,
    pub max_chars: usize,
    pub min_sources: usize,
    pub search_limit: usize,
    pub max_pages: usize,
    pub max_depth: usize,
    pub max_elapsed: Duration,
    pub max_child_candidates: usize,
    pub max_children_per_page: usize,
    pub allow_local: bool,
}

#[derive(Debug, Clone)]
pub struct Source {
    pub url: Url,
    pub trust_tier: TrustTier,
    pub excerpt: String,
}

#[derive(Debug)]
pub struct CrawlResult {
    pub sources: Vec<Source>,
}

#[derive(Debug, Clone)]
struct LinkCandidate {
    url: Url,
    anchor_text: String,
    trust_tier: TrustTier,
}

const MIN_HOST_INTERVAL: Duration = Duration::from_millis(150);
const MAX_EXCERPT_RAW_BYTES: usize = 32 * 1024;
const MAX_EXCERPT_CHARS: usize = 600;
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const USER_AGENT: &str = "llm-spider/0.1 (respectful; contact: unknown)";

#[derive(Default)]
struct Frontier {
    high: VecDeque<(Url, usize)>,
    medium: VecDeque<(Url, usize)>,
    low: VecDeque<(Url, usize)>,
}

impl Frontier {
    fn push(&mut self, url: Url, depth: usize, tier: TrustTier) {
        match tier {
            TrustTier::High => self.high.push_back((url, depth)),
            TrustTier::Medium => self.medium.push_back((url, depth)),
            TrustTier::Low => self.low.push_back((url, depth)),
        }
    }

    fn pop(&mut self) -> Option<(Url, usize)> {
        self.high
            .pop_front()
            .or_else(|| self.medium.pop_front())
            .or_else(|| self.low.pop_front())
    }
}

pub fn crawl(
    request: &UserRequest,
    openai: &crate::openai::OpenAiClient,
) -> anyhow::Result<CrawlResult> {
    let started_at = Instant::now();
    let hits = openai
        .web_search(&request.query, request.search_limit)
        .context("web search")?;

    let runtime = crate::spider_rs::tokio::runtime::Runtime::new()
        .context("build tokio runtime for spider")?;

    let mut frontier = Frontier::default();
    for hit in hits {
        let tier = classify_trust_tier(&hit.url);
        frontier.push(hit.url, 0usize, tier);
    }

    let mut visited = HashSet::<String>::new();
    let mut sources = Vec::<Source>::new();
    let mut last_request_by_host = HashMap::<String, Instant>::new();
    let mut min_interval_by_host = HashMap::<String, Duration>::new();

    while sources.len() < request.max_pages {
        if started_at.elapsed() > request.max_elapsed {
            break;
        }

        let Some((url, depth)) = frontier.pop() else {
            break;
        };

        let normalized = normalize_url(&url);
        if !visited.insert(normalized) {
            continue;
        }

        if !is_allowed(&url, request.allow_local) {
            continue;
        }

        if let Some(host) = url
            .host_str()
            .map(str::to_ascii_lowercase)
            .filter(|host| !host.is_empty())
        {
            let min_interval = min_interval_by_host
                .get(&host)
                .copied()
                .unwrap_or(MIN_HOST_INTERVAL);
            if let Some(last) = last_request_by_host.get(&host) {
                let elapsed = last.elapsed();
                if elapsed < min_interval {
                    std::thread::sleep(min_interval - elapsed);
                }
            }
            last_request_by_host.insert(host, Instant::now());
        }

        let trust_tier = classify_trust_tier(&url);
        let scraped = match scrape_single_page_with_spider(&runtime, &url) {
            Ok(scraped) => scraped,
            Err(err) => {
                warn!(url = %url, "spider fetch failed; skipping: {err:#}");
                continue;
            }
        };

        if let Some(host) = url
            .host_str()
            .map(str::to_ascii_lowercase)
            .filter(|host| !host.is_empty())
        {
            let current = min_interval_by_host.get(&host).copied().unwrap_or_default();
            let updated = current.max(scraped.robots_delay).max(MIN_HOST_INTERVAL);
            min_interval_by_host.insert(host, updated);
        };

        let (excerpt, anchor_text_by_url) =
            match extract_excerpt_and_anchor_map(&url, &scraped.html) {
                Ok(ok) => ok,
                Err(err) => {
                    warn!(url = %url, "extract failed; skipping: {err:#}");
                    continue;
                }
            };

        sources.push(Source {
            url: url.clone(),
            trust_tier,
            excerpt,
        });

        if depth >= request.max_depth {
            continue;
        }

        let mut candidates = Vec::new();
        let mut candidate_seen = HashSet::<String>::new();
        for link_url in scraped.links {
            if !is_allowed(&link_url, request.allow_local) {
                continue;
            }
            let key = normalize_url(&link_url);
            if visited.contains(&key) {
                continue;
            }
            if !candidate_seen.insert(key) {
                continue;
            }

            let anchor_text = anchor_text_by_url
                .get(&normalize_url(&link_url))
                .cloned()
                .unwrap_or_default();
            let trust_tier = classify_trust_tier(&link_url);
            candidates.push(LinkCandidate {
                url: link_url,
                anchor_text,
                trust_tier,
            });
            if candidates.len() >= request.max_child_candidates {
                break;
            }
        }

        if request.max_children_per_page == 0 || candidates.is_empty() {
            continue;
        }

        candidates.sort_by(|a, b| {
            a.trust_tier
                .cmp(&b.trust_tier)
                .then_with(|| a.url.as_str().cmp(b.url.as_str()))
        });

        let candidate_values = candidates
            .iter()
            .map(|c| {
                json!({
                    "url": c.url.as_str(),
                    "anchor_text": c.anchor_text,
                    "trust_tier": format!("{:?}", c.trust_tier),
                })
            })
            .collect::<Vec<Value>>();

        let selected = if candidates.len() <= request.max_children_per_page {
            candidates
                .iter()
                .take(request.max_children_per_page)
                .map(|c| c.url.clone())
                .collect::<Vec<_>>()
        } else {
            openai
                .select_child_links(
                    &request.query,
                    &url,
                    sources.last().map(|s| s.excerpt.as_str()).unwrap_or(""),
                    &candidate_values,
                    request.max_children_per_page,
                )
                .with_context(|| format!("select child links: {url}"))?
        };

        for child_url in selected {
            if !is_allowed(&child_url, request.allow_local) {
                continue;
            }
            let child_tier = classify_trust_tier(&child_url);
            frontier.push(child_url, depth + 1, child_tier);
        }
    }

    Ok(CrawlResult { sources })
}

pub fn compose_markdown(request: &UserRequest, result: &CrawlResult) -> String {
    let mut out = String::new();
    out.push_str("# Spider Result\n\n");
    out.push_str("## Query\n\n");
    out.push_str("- ");
    out.push_str(&escape_md_inline(&request.query));
    out.push('\n');
    out.push('\n');

    out.push_str("## Findings\n\n");
    if result.sources.is_empty() {
        out.push_str("- No sources collected.\n");
    } else {
        for source in &result.sources {
            out.push_str("- ");
            out.push_str(&format!(
                "[{:?}] {}",
                source.trust_tier,
                escape_md_inline(source.url.as_str())
            ));
            out.push('\n');
            if !source.excerpt.is_empty() {
                out.push_str("  - ");
                out.push_str(&escape_md_inline(&source.excerpt));
                out.push('\n');
            }
        }
    }
    out.push('\n');

    out.push_str("## Sources\n\n");
    for source in &result.sources {
        out.push_str("- ");
        out.push_str(&format!("[{:?}] {}", source.trust_tier, source.url));
        out.push('\n');
    }

    if result.sources.len() < request.min_sources {
        out.push('\n');
        out.push_str("## Notes\n\n");
        out.push_str("- `min_sources` を満たせなかった。\n");
        out.push_str("- 収集制約（`max_pages` / `max_depth` / `max_elapsed`）を見直す。\n");
    }

    truncate_to_char_limit(out, request.max_chars)
}

fn normalize_url(url: &Url) -> String {
    let mut normalized = url.clone();
    normalized.set_fragment(None);
    normalized.to_string()
}

fn is_allowed(url: &Url, allow_local: bool) -> bool {
    match url.scheme() {
        "http" | "https" => {}
        _ => return false,
    }
    let Some(host) = url.host() else {
        return false;
    };
    if allow_local {
        return true;
    }

    match host {
        url::Host::Domain(domain) => {
            let domain = domain.to_ascii_lowercase();
            domain != "localhost" && !domain.ends_with(".localhost")
        }
        url::Host::Ipv4(ip) => !is_local_ipv4(ip),
        url::Host::Ipv6(ip) => !ip.is_loopback() && !ip.is_unique_local() && !ip.is_unspecified(),
    }
}

fn is_local_ipv4(ip: std::net::Ipv4Addr) -> bool {
    ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_unspecified()
}

#[derive(Debug)]
struct SpiderScrape {
    html: String,
    links: Vec<Url>,
    robots_delay: Duration,
}

fn scrape_single_page_with_spider(
    runtime: &crate::spider_rs::tokio::runtime::Runtime,
    url: &Url,
) -> anyhow::Result<SpiderScrape> {
    let mut website = crate::spider_rs::website::Website::new(url.as_str());
    website
        .with_respect_robots_txt(true)
        .with_user_agent(Some(USER_AGENT))
        .with_request_timeout(Some(DEFAULT_REQUEST_TIMEOUT))
        .with_max_bytes_allowed(Some(MAX_RESPONSE_BYTES as u64))
        .with_external_domains(Some(std::iter::once("*".to_owned())))
        .with_limit(1);

    let (client, control) = runtime.block_on(async { website.setup().await });

    let robots_delay = website.get_delay();

    if !website.is_allowed_robots(url.as_str()) {
        anyhow::bail!("blocked by robots.txt");
    }

    let mut page = runtime
        .block_on(async { crate::spider_rs::page::Page::new_page(url.as_str(), &client).await });

    if let Some((_state, join)) = control {
        join.abort();
    }

    if !page.status_code.is_success() {
        anyhow::bail!("http status: {}", page.status_code);
    }

    page.set_external(website.configuration.external_domains_caseless.clone());

    let selectors = crate::spider_rs::page::get_page_selectors(url.as_str(), true, true);
    let base = Some(Box::new(url.clone()));
    let links = runtime.block_on(async { page.links(&selectors, &base).await });

    let mut out_links = Vec::<Url>::new();
    for link in links {
        let Ok(mut parsed) = Url::parse(link.as_ref()) else {
            continue;
        };
        parsed.set_fragment(None);
        if matches!(parsed.scheme(), "http" | "https") {
            out_links.push(parsed);
        }
    }
    out_links.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    out_links.dedup_by(|a, b| a.as_str() == b.as_str());

    Ok(SpiderScrape {
        html: page.get_html(),
        links: out_links,
        robots_delay,
    })
}

fn extract_excerpt_and_anchor_map(
    base_url: &Url,
    html: &str,
) -> anyhow::Result<(String, HashMap<String, String>)> {
    let cleaned_html = strip_tag_blocks(html, "noscript");
    let doc = Html::parse_document(&cleaned_html);

    let content_root = select_content_root(&doc)?;
    let link_selector = Selector::parse("a[href]")
        .map_err(|err| anyhow::anyhow!("parse selector a[href]: {err:?}"))?;

    let mut raw_text = String::new();
    for piece in content_root.text() {
        raw_text.push_str(piece);
        raw_text.push(' ');
        if raw_text.len() >= MAX_EXCERPT_RAW_BYTES {
            break;
        }
    }
    let excerpt = truncate_chars(&normalize_text(&raw_text), MAX_EXCERPT_CHARS);

    let mut anchor_text_by_url = HashMap::<String, String>::new();
    for node in content_root.select(&link_selector) {
        let Some(href) = node.value().attr("href") else {
            continue;
        };
        let Ok(mut url) = base_url.join(href) else {
            continue;
        };
        url.set_fragment(None);
        if matches!(url.scheme(), "http" | "https") {
            let anchor_text = node.text().collect::<Vec<_>>().join(" ");
            let anchor_text = truncate_chars(&normalize_text(&anchor_text), 120);
            if anchor_text.is_empty() {
                continue;
            }
            let normalized = normalize_url(&url);
            anchor_text_by_url.entry(normalized).or_insert(anchor_text);
        }
    }

    Ok((excerpt, anchor_text_by_url))
}

fn strip_tag_blocks(html: &str, tag_name: &str) -> String {
    let tag_lc = tag_name.to_ascii_lowercase();
    let open_pat = format!("<{tag_lc}");
    let close_pat = format!("</{tag_lc}>");

    let lower = html.to_ascii_lowercase();
    let mut out = String::with_capacity(html.len());
    let mut idx = 0usize;

    while let Some(open_rel) = lower[idx..].find(&open_pat) {
        let open_idx = idx + open_rel;
        out.push_str(&html[idx..open_idx]);

        let search_from = open_idx + open_pat.len();
        let Some(close_rel) = lower[search_from..].find(&close_pat) else {
            idx = html.len();
            break;
        };
        let close_end = search_from + close_rel + close_pat.len();
        idx = close_end;
    }

    out.push_str(&html[idx..]);
    out
}

fn select_content_root<'a>(doc: &'a Html) -> anyhow::Result<ElementRef<'a>> {
    for selector in ["main", "article", "[role=main]", "body"] {
        let selector = Selector::parse(selector)
            .map_err(|err| anyhow::anyhow!("parse selector {selector}: {err:?}"))?;
        if let Some(root) = doc.select(&selector).next() {
            return Ok(root);
        }
    }

    anyhow::bail!("missing content root");
}

fn normalize_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

fn truncate_to_char_limit(mut text: String, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    if text.chars().count() <= max_chars {
        return text;
    }

    let mut end_byte = 0usize;
    for (i, (byte_idx, _)) in text.char_indices().enumerate() {
        if i == max_chars {
            end_byte = byte_idx;
            break;
        }
    }
    if end_byte == 0 {
        end_byte = text.len();
    }
    text.truncate(end_byte);
    text
}

fn escape_md_inline(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]")
        .replace('`', "\\`")
}
