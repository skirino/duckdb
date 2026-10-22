//! Dead-node counter bookkeeping for the eviction queue.

use std::sync::atomic::{AtomicU64, Ordering};

use super::idx_t;

pub struct DeadNodeCounter {
    value: AtomicU64,
}

impl Default for DeadNodeCounter {
    fn default() -> Self {
        Self::new()
    }
}

impl DeadNodeCounter {
    pub fn new() -> Self {
        Self {
            value: AtomicU64::new(0),
        }
    }

    pub fn increment(&self) -> idx_t {
        let mut current = self.value.load(Ordering::Relaxed);
        loop {
            let new_val = current.saturating_add(1);
            match self.value.compare_exchange_weak(
                current,
                new_val,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return new_val,
                Err(actual) => current = actual,
            }
        }
    }

    pub fn decrement(&self) -> idx_t {
        let mut current = self.value.load(Ordering::Relaxed);
        loop {
            let new_val = current.saturating_sub(1);
            match self.value.compare_exchange_weak(
                current,
                new_val,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return new_val,
                Err(actual) => current = actual,
            }
        }
    }

    pub fn subtract(&self, n: idx_t) -> idx_t {
        let mut current = self.value.load(Ordering::Relaxed);
        loop {
            let new_val = current.saturating_sub(n);
            match self.value.compare_exchange_weak(
                current,
                new_val,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return new_val,
                Err(actual) => current = actual,
            }
        }
    }

    pub fn get(&self) -> idx_t {
        self.value.load(Ordering::Relaxed)
    }
}

pub fn new_dead_counter() -> *mut DeadNodeCounter {
    Box::into_raw(Box::new(DeadNodeCounter::new()))
}

/// # Safety
/// `ptr` must come from [`new_dead_counter`].
pub unsafe fn delete_dead_counter(ptr: *mut DeadNodeCounter) {
    if !ptr.is_null() {
        // SAFETY: `ptr` came from `Box::into_raw`.
        unsafe {
            drop(Box::from_raw(ptr));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::thread;

    use super::*;

    #[test]
    fn increment_decrement_never_wrap() {
        let c = DeadNodeCounter::new();
        for _ in 0..5 {
            c.increment();
        }
        assert_eq!(c.get(), 5);
        for _ in 0..10 {
            c.decrement();
        }
        assert_eq!(c.get(), 0);
    }

    #[test]
    fn subtract_saturates() {
        let c = DeadNodeCounter::new();
        c.increment();
        c.subtract(5);
        assert_eq!(c.get(), 0);
    }

    #[test]
    fn concurrent_no_wrap() {
        let c = Arc::new(DeadNodeCounter::new());
        let mut handles = vec![];
        for _ in 0..4 {
            let c = Arc::clone(&c);
            handles.push(thread::spawn(move || {
                for _ in 0..1000 {
                    c.increment();
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(c.get(), 4000);
        // decrements from many threads
        let mut handles = vec![];
        for _ in 0..4 {
            let c = Arc::clone(&c);
            handles.push(thread::spawn(move || {
                for _ in 0..1000 {
                    c.decrement();
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(c.get(), 0);
    }
}
