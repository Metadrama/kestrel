pub mod hls;
pub mod social;
pub mod ytdlp;

use async_trait::async_trait;
use kestrel_proto::{KestrelError, MediaVariant, MimeCategory};
use reqwest::header::HeaderMap;
use url::Url;

#[derive(Debug, Clone)]
pub struct ResolvedMedia {
    pub title: String,
    pub mime_category: MimeCategory,
    pub variants: Vec<MediaVariant>,
    pub direct_download_url: Option<String>,
    pub audio_url: Option<String>,
    pub is_hls_stream: bool,
    pub hls_segments: Vec<String>,
}

#[async_trait]
pub trait LinkResolver: Send + Sync {
    fn priority(&self) -> u8;
    fn can_handle(&self, url: &Url) -> bool;
    async fn resolve(&self, url: &Url, headers: &HeaderMap) -> Result<ResolvedMedia, KestrelError>;
}

pub struct ResolverRegistry {
    resolvers: Vec<Box<dyn LinkResolver>>,
}

impl ResolverRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            resolvers: Vec::new(),
        };
        registry.register(Box::new(hls::HlsDashResolver::new()));
        registry.register(Box::new(social::SocialNativeResolver::new()));
        registry.register(Box::new(ytdlp::YtDlpResolver::new()));
        registry.resolvers.sort_by_key(|r| std::cmp::Reverse(r.priority()));
        registry
    }
}

impl Default for ResolverRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ResolverRegistry {

    pub fn register(&mut self, resolver: Box<dyn LinkResolver>) {
        self.resolvers.push(resolver);
        self.resolvers.sort_by_key(|r| std::cmp::Reverse(r.priority()));
    }

    pub async fn resolve(&self, url_str: &str, headers: &HeaderMap) -> Option<Result<ResolvedMedia, KestrelError>> {
        let parsed = Url::parse(url_str).ok()?;
        for r in &self.resolvers {
            if r.can_handle(&parsed) {
                return Some(r.resolve(&parsed, headers).await);
            }
        }
        None
    }
}
