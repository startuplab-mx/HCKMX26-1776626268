use bytes::Bytes;
use dashmap::DashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use tracing::{debug, warn};

const SOFT_MAX_BYTES: usize = 256 * 1024 * 1024;

#[derive(Clone)]
pub struct CachedAsset {
    pub status: u16,
    pub content_type: String,
    pub body: Bytes,
}

pub struct AssetCache {
    map: DashMap<String, CachedAsset>,
    bytes: AtomicUsize,
}

impl AssetCache {
    pub fn new() -> Self {
        Self {
            map: DashMap::new(),
            bytes: AtomicUsize::new(0),
        }
    }

    pub fn insert(&self, url: String, asset: CachedAsset) {
        let size = asset.body.len();
        if let Some(prev) = self.map.insert(url, asset) {
            self.bytes.fetch_sub(prev.body.len(), Ordering::Relaxed);
        }
        let total = self.bytes.fetch_add(size, Ordering::Relaxed) + size;
        debug!(total_bytes = total, "asset cache inserted");
        if total > SOFT_MAX_BYTES {
            warn!(
                total_bytes = total,
                limit = SOFT_MAX_BYTES,
                "asset cache exceeded soft limit; consider clearing"
            );
        }
    }

    pub fn get(&self, url: &str) -> Option<CachedAsset> {
        self.map.get(url).map(|e| e.clone())
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.map.len()
    }
}
