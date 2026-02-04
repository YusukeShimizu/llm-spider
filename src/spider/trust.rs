use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TrustTier {
    High,
    Medium,
    Low,
}

pub fn classify_trust_tier(url: &Url) -> TrustTier {
    let Some(host) = url.host_str() else {
        return TrustTier::Low;
    };
    let host = host.to_ascii_lowercase();

    if host == "reddit.com"
        || host.ends_with(".reddit.com")
        || host == "x.com"
        || host.ends_with(".x.com")
    {
        return TrustTier::Low;
    }

    if host == "rust-lang.org"
        || host.ends_with(".rust-lang.org")
        || host == "doc.rust-lang.org"
        || host == "docs.rs"
    {
        return TrustTier::High;
    }

    if host.ends_with(".gov")
        || host.contains(".gov.")
        || host.ends_with(".edu")
        || host.contains(".edu.")
        || host.ends_with(".ac.jp")
        || host.contains(".ac.")
        || host.ends_with(".go.jp")
        || host.contains(".go.jp")
    {
        return TrustTier::High;
    }

    TrustTier::Medium
}
