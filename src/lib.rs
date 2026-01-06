// This code is forked from https://docs.rs/dashmap/7.0.0-rc2/src/dashmap/lib.rs.html,
// available under the MIT license.

//! A fork of the [`dashmap`](https://docs.rs/dashmap) crate to expose a raw hash table API.

#![forbid(
    missing_docs,
    unsafe_op_in_unsafe_fn,
    clippy::missing_safety_doc,
    clippy::multiple_unsafe_ops_per_block,
    clippy::undocumented_unsafe_blocks
)]

mod lock;

use crossbeam_utils::CachePadded;
use hashbrown::{HashTable, hash_table};
use lock::{RwLock, RwLockWriteGuardDetached};
use std::sync::LazyLock;

/// A concurrent raw hash table with items of type `T`.
pub struct DashTable<T> {
    shift: u32,
    shards: Box<[CachePadded<RwLock<HashTable<T>>>]>,
}

fn default_shard_shift() -> u32 {
    static DEFAULT_SHARD_SHIFT: LazyLock<u32> = LazyLock::new(|| {
        (std::thread::available_parallelism().map_or(1, usize::from) * 4)
            .next_power_of_two()
            .ilog2()
    });
    *DEFAULT_SHARD_SHIFT
}

impl<T> Default for DashTable<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> DashTable<T> {
    /// Creates a new concurrent raw hash table.
    pub fn new() -> Self {
        let shard_shift = default_shard_shift();
        assert!(shard_shift > 0);
        let shard_amount = 1 << shard_shift;

        let shift = usize::BITS - shard_shift;

        let shards = (0..shard_amount)
            .map(|_| CachePadded::new(RwLock::new(HashTable::new())))
            .collect();

        Self { shift, shards }
    }

    /// Creates a new concurrent raw hash table, pre-allocating capacity for
    /// approximately the given number of items
    pub fn with_capacity(capacity: usize) -> Self {
        let shard_shift = default_shard_shift();
        assert!(shard_shift > 0);
        let shard_amount = 1 << shard_shift;

        let shift = usize::BITS - shard_shift;

        let cps = (capacity + (shard_amount - 1)) >> shard_shift;

        let shards = (0..shard_amount)
            .map(|_| CachePadded::new(RwLock::new(HashTable::with_capacity(cps))))
            .collect();

        Self { shift, shards }
    }

    /// Retrieves an entry for the given hash value.
    pub fn entry<'a>(
        &'a self,
        hash: u64,
        eq: impl FnMut(&T) -> bool,
        hasher: impl Fn(&T) -> u64,
    ) -> Entry<'a, T> {
        let shard = self.determine_shard(hash as usize);
        let guard = self.shards[shard].write();
        // SAFETY: The data doesn't outlive the detached guard, as the guard is stored
        // in the entry returned by this function, and the entry properly ties
        // the guard's lifetime to the corresponding value.
        let (_guard, shard) = unsafe { RwLockWriteGuardDetached::detach_from(guard) };

        match shard.entry(hash, eq, hasher) {
            hash_table::Entry::Occupied(entry) => Entry::Occupied(OccupiedEntry { _guard, entry }),
            hash_table::Entry::Vacant(entry) => Entry::Vacant(VacantEntry { _guard, entry }),
        }
    }

    /// Unconditionally inserts the given value for the given hash, without
    /// checking whether an equivalent element already exists in the table.
    pub fn insert_unique<'a>(
        &'a self,
        hash: u64,
        value: T,
        hasher: impl Fn(&T) -> u64,
    ) -> OccupiedEntry<'a, T> {
        let shard = self.determine_shard(hash as usize);
        let guard = self.shards[shard].write();
        // SAFETY: The data doesn't outlive the detached guard, as the guard is stored
        // in the entry returned by this function, and the entry properly ties
        // the guard's lifetime to the corresponding value.
        let (_guard, shard) = unsafe { RwLockWriteGuardDetached::detach_from(guard) };

        let entry = shard.insert_unique(hash, value, hasher);
        OccupiedEntry { _guard, entry }
    }

    fn determine_shard(&self, hash: usize) -> usize {
        // Leave the high 7 bits for the HashBrown SIMD tag.
        (hash << 7) >> self.shift
    }
}

/// A view into a single entry in a table, which may either be vacant or
/// occupied.
pub enum Entry<'a, T> {
    /// The entry contains a value.
    Occupied(OccupiedEntry<'a, T>),
    /// The entry doesn't contain any value.
    Vacant(VacantEntry<'a, T>),
}

impl<'a, T> Entry<'a, T> {
    /// If this entry is vacant, inserts the given default value into it.
    pub fn or_insert_with(self, default: impl FnOnce() -> T) -> OccupiedEntry<'a, T> {
        match self {
            Entry::Occupied(entry) => entry,
            Entry::Vacant(entry) => entry.insert(default()),
        }
    }
}

/// A hash table entry that contains a value.
pub struct OccupiedEntry<'a, T> {
    _guard: RwLockWriteGuardDetached<'a>,
    entry: hash_table::OccupiedEntry<'a, T>,
}

impl<T> OccupiedEntry<'_, T> {
    /// Obtains a reference to the corresponding value.
    pub fn get(&self) -> &T {
        self.entry.get()
    }
}

/// A hash table entry that doesn't contain any value.
pub struct VacantEntry<'a, T> {
    _guard: RwLockWriteGuardDetached<'a>,
    entry: hash_table::VacantEntry<'a, T>,
}

impl<'a, T> VacantEntry<'a, T> {
    /// Inserts the given value into this entry.
    pub fn insert(self, value: T) -> OccupiedEntry<'a, T> {
        OccupiedEntry {
            _guard: self._guard,
            entry: self.entry.insert(value),
        }
    }
}
