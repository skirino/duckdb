//! Cached-counter flush behind `BufferPool::MemoryUsage`.

use std::sync::atomic::{AtomicI64, Ordering};

use super::idx_t;

pub const MEMORY_USAGE_CACHE_COUNT: usize = 64;
pub const MEMORY_USAGE_CACHE_THRESHOLD: idx_t = 32 << 10;
pub const MEMORY_TAG_COUNT: usize = 16;
pub const TOTAL_MEMORY_USAGE_INDEX: usize = MEMORY_TAG_COUNT;

pub struct MemoryUsage {
    memory_usage: [AtomicI64; MEMORY_TAG_COUNT + 1],
    memory_usage_caches: [[AtomicI64; MEMORY_TAG_COUNT + 1]; MEMORY_USAGE_CACHE_COUNT],
}

impl Default for MemoryUsage {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryUsage {
    pub fn new() -> Self {
        Self {
            memory_usage: std::array::from_fn(|_| AtomicI64::new(0)),
            memory_usage_caches: std::array::from_fn(|_| {
                std::array::from_fn(|_| AtomicI64::new(0))
            }),
        }
    }

    pub fn get_used_memory(&self, index: idx_t, flush: bool) -> idx_t {
        let idx = index as usize;
        if !flush {
            let v = self.memory_usage[idx].load(Ordering::Relaxed);
            return if v > 0 { v as idx_t } else { 0 };
        }
        let mut cached: i64 = 0;
        for cache in &self.memory_usage_caches {
            cached += cache[idx].swap(0, Ordering::Relaxed);
        }
        let v = self.memory_usage[idx].fetch_add(cached, Ordering::Relaxed) + cached;
        if v > 0 { v as idx_t } else { 0 }
    }

    pub fn update_used_memory(&self, tag: u8, size: i64, cache_idx: idx_t) {
        let tag_idx = tag as usize;
        let total_idx = TOTAL_MEMORY_USAGE_INDEX;
        if (size.unsigned_abs()) < MEMORY_USAGE_CACHE_THRESHOLD {
            let ci = (cache_idx as usize) % MEMORY_USAGE_CACHE_COUNT;
            let cache = &self.memory_usage_caches[ci];
            let new_tag = cache[tag_idx].fetch_add(size, Ordering::Relaxed) + size;
            if (new_tag.unsigned_abs()) >= MEMORY_USAGE_CACHE_THRESHOLD {
                let tag_size = cache[tag_idx].swap(0, Ordering::Relaxed);
                self.memory_usage[tag_idx].fetch_add(tag_size, Ordering::Relaxed);
            }
            let new_total = cache[total_idx].fetch_add(size, Ordering::Relaxed) + size;
            if (new_total.unsigned_abs()) >= MEMORY_USAGE_CACHE_THRESHOLD {
                let total_size = cache[total_idx].swap(0, Ordering::Relaxed);
                self.memory_usage[total_idx].fetch_add(total_size, Ordering::Relaxed);
            }
        } else {
            self.memory_usage[tag_idx].fetch_add(size, Ordering::Relaxed);
            self.memory_usage[total_idx].fetch_add(size, Ordering::Relaxed);
        }
    }
}

/// Creates a heap-allocated `MemoryUsage`.
pub fn new_memory_usage() -> *mut MemoryUsage {
    Box::into_raw(Box::new(MemoryUsage::new()))
}

/// Deletes a `MemoryUsage` created by [`new_memory_usage`].
///
/// # Safety
/// `ptr` must come from [`new_memory_usage`] and must not be used afterwards.
pub unsafe fn delete_memory_usage(ptr: *mut MemoryUsage) {
    if !ptr.is_null() {
        // SAFETY: `ptr` came from `Box::into_raw` in `new_memory_usage`.
        unsafe {
            drop(Box::from_raw(ptr));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_and_flush_round_trip() {
        let u = MemoryUsage::new();
        u.update_used_memory(0, 1000, 0);
        // small, cached; flush moves it
        let v = u.get_used_memory(TOTAL_MEMORY_USAGE_INDEX as idx_t, true);
        assert_eq!(v, 1000);
    }

    #[test]
    fn large_update_hits_global_directly() {
        let u = MemoryUsage::new();
        let big = (MEMORY_USAGE_CACHE_THRESHOLD as i64) + 10;
        u.update_used_memory(1, big, 3);
        let v = u.get_used_memory(1, false);
        assert_eq!(v, big as idx_t);
    }

    #[test]
    fn no_flush_keeps_cached() {
        let u = MemoryUsage::new();
        u.update_used_memory(0, 100, 0);
        // no flush: global may be 0
        let _ = u.get_used_memory(TOTAL_MEMORY_USAGE_INDEX as idx_t, false);
    }
}
