//! Per-VM bounded cache of immutable engines. Ruby Regexp instances and their
//! capture tables remain independent; only the compiled matcher is shared.
use super::{CompileError, Pattern, compile};
use alloc::{sync::Arc, vec::Vec};
use hashbrown::{Equivalent, HashMap};

const CAPACITY: usize = 128;
const MAX_SOURCE_BYTES: usize = 4096;

#[derive(Hash, PartialEq, Eq)]
struct Key {
    source: Vec<u8>,
    flags: u32,
    binary: bool,
}

#[derive(Hash)]
struct BorrowedKey<'a> {
    source: &'a [u8],
    flags: u32,
    binary: bool,
}

impl Equivalent<Key> for BorrowedKey<'_> {
    fn equivalent(&self, key: &Key) -> bool {
        self.source == key.source && self.flags == key.flags && self.binary == key.binary
    }
}

#[derive(Default)]
pub(crate) struct PatternCache {
    entries: HashMap<Key, (Arc<Pattern>, u64)>,
    clock: u64,
}

impl PatternCache {
    pub fn compile(
        &mut self,
        source: &[u8],
        flags: u32,
        binary: bool,
    ) -> Result<Arc<Pattern>, CompileError> {
        self.clock = self.clock.wrapping_add(1);
        let key = BorrowedKey {
            source,
            flags,
            binary,
        };
        if let Some((pattern, used)) = self.entries.get_mut(&key) {
            *used = self.clock;
            return Ok(Arc::clone(pattern));
        }
        let pattern = Arc::new(compile(source, flags, binary)?);
        if source.len() <= MAX_SOURCE_BYTES {
            if self.entries.len() >= CAPACITY {
                // Misses are rare for game literals; avoid a list or allocation
                // on the hot hit path just to maintain a 128-entry LRU.
                if let Some(oldest) =
                    self.entries
                        .iter()
                        .min_by_key(|(_, (_, used))| *used)
                        .map(|(key, _)| Key {
                            source: key.source.clone(),
                            flags: key.flags,
                            binary: key.binary,
                        })
                {
                    self.entries.remove(&oldest);
                }
            }
            self.entries.insert(
                Key {
                    source: source.to_vec(),
                    flags,
                    binary,
                },
                (Arc::clone(&pattern), self.clock),
            );
        }
        Ok(pattern)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn engines_are_shared_only_for_equal_sources_flags_and_encodings() {
        let mut cache = PatternCache::default();
        let first = cache.compile(b"hello", 0, false).unwrap();
        assert!(Arc::ptr_eq(
            &first,
            &cache.compile(b"hello", 0, false).unwrap()
        ));
        assert!(!Arc::ptr_eq(
            &first,
            &cache.compile(b"hello", 1, false).unwrap()
        ));
        assert!(!Arc::ptr_eq(
            &first,
            &cache.compile(b"hello", 0, true).unwrap()
        ));
        for i in 0..200 {
            cache
                .compile(alloc::format!("pattern{i}").as_bytes(), 0, false)
                .unwrap();
        }
        assert_eq!(cache.entries.len(), CAPACITY);
        assert!(!Arc::ptr_eq(
            &first,
            &cache.compile(b"hello", 0, false).unwrap()
        ));
        assert!(super::super::exec(&first, b"hello", 0, true).is_some());
    }
}
