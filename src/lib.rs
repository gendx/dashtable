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
#![cfg_attr(docsrs, feature(doc_cfg))]

mod lock;

use crossbeam_utils::CachePadded;
#[cfg(feature = "get-size2")]
use get_size2::{GetSize, GetSizeTracker};
use hashbrown::{HashTable, hash_table};
use lock::{RwLock, RwLockReadGuardDetached, RwLockWriteGuardDetached};
use std::ops::Deref;
use std::sync::LazyLock;

fn default_shard_shift() -> u32 {
    static DEFAULT_SHARD_SHIFT: LazyLock<u32> =
        LazyLock::new(|| shard_shift(std::thread::available_parallelism().map_or(1, usize::from)));
    *DEFAULT_SHARD_SHIFT
}

fn shard_shift(parallelism: usize) -> u32 {
    if parallelism == 0 {
        1
    } else {
        ((parallelism.saturating_mul(4) - 1).ilog2() + 1).min(usize::BITS - 1)
    }
}

fn capacity_per_shard(capacity: usize, shard_shift: u32) -> usize {
    let shard_amount = 1 << shard_shift;
    capacity.div_ceil(shard_amount)
}

/// A concurrent raw hash table with items of type `T`.
pub struct DashTable<T> {
    shift: u32,
    shards: Box<[CachePadded<RwLock<HashTable<T>>>]>,
}

impl<T> Default for DashTable<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "get-size2")]
impl<T: GetSize> GetSize for DashTable<T> {
    fn get_heap_size_with_tracker<Tr: GetSizeTracker>(&self, tracker: Tr) -> (usize, Tr) {
        let (size, tracker) = self
            .shards
            .iter()
            .fold((0, tracker), |(size, tracker), table| {
                let (table_size, tracker) =
                    HashTable::get_size_with_tracker(table.read().deref(), tracker);
                (size + table_size, tracker)
            });

        let allocation_size =
            self.shards.len() * std::mem::size_of::<CachePadded<RwLock<HashTable<T>>>>();
        (size + allocation_size, tracker)
    }
}

impl<T> DashTable<T> {
    /// Creates a new concurrent raw hash table.
    pub fn new() -> Self {
        let shard_shift = default_shard_shift();
        assert!(shard_shift > 0);
        assert!(shard_shift < usize::BITS);
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
        assert!(shard_shift < usize::BITS);
        let shard_amount = 1 << shard_shift;

        let shift = usize::BITS - shard_shift;

        let cps = capacity_per_shard(capacity, shard_shift);

        let shards = (0..shard_amount)
            .map(|_| CachePadded::new(RwLock::new(HashTable::with_capacity(cps))))
            .collect();

        Self { shift, shards }
    }

    /// Retrieves an entry for the given hash value.
    ///
    /// See also:
    /// - [`entry_mut()`](Self::entry_mut), which is more efficient if you hold
    ///   a mutable reference to this [`DashTable`], as it avoids acquiring
    ///   locks,
    /// - [`find()`](Self::find), which is more efficient if you don't need to
    ///   modify the entry, as it only acquires a read lock.
    pub fn entry<'a>(
        &'a self,
        hash: u64,
        eq: impl FnMut(&T) -> bool,
        hasher: impl Fn(&T) -> u64,
    ) -> Entry<'a, T> {
        let shard = self.determine_shard(hash as usize);
        let guard = self.shards[shard].write();
        // SAFETY: The data doesn't outlive the detached guard, as the guard is
        // stored in the entry returned by this function, and the entry properly
        // ties the guard's lifetime to the corresponding value.
        let (_guard, shard) = unsafe { guard.detach() };

        match shard.entry(hash, eq, hasher) {
            hash_table::Entry::Occupied(entry) => Entry::Occupied(OccupiedEntry { _guard, entry }),
            hash_table::Entry::Vacant(entry) => Entry::Vacant(VacantEntry { _guard, entry }),
        }
    }

    /// Retrieves an entry for the given hash value.
    ///
    /// Contrary to [`entry()`](Self::entry), no lock is held internally because
    /// this function already takes an exclusive mutable reference to this
    /// [`DashTable`].
    pub fn entry_mut<'a>(
        &'a mut self,
        hash: u64,
        eq: impl FnMut(&T) -> bool,
        hasher: impl Fn(&T) -> u64,
    ) -> MutEntry<'a, T> {
        let shard = self.determine_shard(hash as usize);
        let table = self.shards[shard].get_mut();
        table.entry(hash, eq, hasher)
    }

    /// Returns a reference to a value if one matches the given hash.
    ///
    /// This is more efficient than using [`entry()`](Self::entry) as this only
    /// uses a read lock.
    ///
    /// See also [`find_mut`](Self::find_mut), which is more efficient if you
    /// hold a mutable reference to this [`DashTable`] as it avoids acquiring
    /// locks.
    pub fn find<'a>(&'a self, hash: u64, eq: impl FnMut(&T) -> bool) -> Option<Ref<'a, T>> {
        let shard = self.determine_shard(hash as usize);
        let guard = self.shards[shard].read();
        // SAFETY: The data doesn't outlive the detached guard, as the guard is
        // stored in the entry returned by this function, and the entry properly
        // ties the guard's lifetime to the corresponding value.
        let (_guard, shard) = unsafe { guard.detach() };

        shard
            .find(hash, eq)
            .map(|reference| Ref { _guard, reference })
    }

    /// Returns a reference to a value if one matches the given hash.
    ///
    /// Contrary to [`find()`](Self::find), no lock is held internally because
    /// this function already takes an exclusive mutable reference to this
    /// [`DashTable`].
    pub fn find_mut(&mut self, hash: u64, eq: impl FnMut(&T) -> bool) -> Option<&T> {
        let shard = self.determine_shard(hash as usize);
        let table = self.shards[shard].get_mut();
        table.find(hash, eq)
    }

    /// Unconditionally inserts the given value for the given hash, without
    /// checking whether an equivalent element already exists in the table.
    ///
    /// See also [`insert_unique_mut`](Self::insert_unique_mut), which is more
    /// efficient if you hold a mutable reference to this [`DashTable`] as
    /// it avoids acquiring locks.
    pub fn insert_unique<'a>(
        &'a self,
        hash: u64,
        value: T,
        hasher: impl Fn(&T) -> u64,
    ) -> OccupiedEntry<'a, T> {
        let shard = self.determine_shard(hash as usize);
        let guard = self.shards[shard].write();
        // SAFETY: The data doesn't outlive the detached guard, as the guard is
        // stored in the entry returned by this function, and the entry properly
        // ties the guard's lifetime to the corresponding value.
        let (_guard, shard) = unsafe { guard.detach() };

        let entry = shard.insert_unique(hash, value, hasher);
        OccupiedEntry { _guard, entry }
    }

    /// Unconditionally inserts the given value for the given hash, without
    /// checking whether an equivalent element already exists in the table.
    ///
    /// Contrary to [`insert_unique()`](Self::insert_unique), no lock is held
    /// internally because this function already takes an exclusive mutable
    /// reference to this [`DashTable`].
    pub fn insert_unique_mut<'a>(
        &'a mut self,
        hash: u64,
        value: T,
        hasher: impl Fn(&T) -> u64,
    ) -> MutOccupiedEntry<'a, T> {
        let shard = self.determine_shard(hash as usize);
        let table = self.shards[shard].get_mut();
        table.insert_unique(hash, value, hasher)
    }

    fn determine_shard(&self, hash: usize) -> usize {
        // Leave the high 7 bits for the HashBrown SIMD tag.
        (hash << 7) >> self.shift
    }
}

/// Read-only reference to a value in a table.
pub struct Ref<'a, T> {
    _guard: RwLockReadGuardDetached<'a>,
    reference: &'a T,
}

impl<'a, T> Deref for Ref<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.reference
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

impl<'a, T> OccupiedEntry<'a, T> {
    /// Obtains a reference to the corresponding value.
    pub fn get(&self) -> &T {
        self.entry.get()
    }

    /// Obtains a mutable reference to the corresponding value.
    pub fn get_mut(&mut self) -> &mut T {
        self.entry.get_mut()
    }

    /// Removes the value out of the entry, and returns it along with a
    /// [`VacantEntry`] that can be used to insert another value with the same
    /// hash.
    pub fn remove(self) -> (T, VacantEntry<'a, T>) {
        let (t, entry) = self.entry.remove();
        (
            t,
            VacantEntry {
                _guard: self._guard,
                entry,
            },
        )
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

/// A view into a single entry in a mutable table, which may either be vacant or
/// occupied.
pub type MutEntry<'a, T> = hash_table::Entry<'a, T>;

/// An entry in a mutable hash table that contains a value.
pub type MutOccupiedEntry<'a, T> = hash_table::OccupiedEntry<'a, T>;

#[cfg(test)]
mod test {
    use super::*;
    use std::hash::{BuildHasher, RandomState};
    use std::thread;

    #[test]
    fn test_shard_shift() {
        assert_eq!(shard_shift(0), 1);
        assert_eq!(shard_shift(1), 2);
        assert_eq!(shard_shift(2), 3);
        assert_eq!(shard_shift(3), 4);
        assert_eq!(shard_shift(4), 4);
        assert_eq!(shard_shift(5), 5);
        assert_eq!(shard_shift(6), 5);
        assert_eq!(shard_shift(7), 5);
        assert_eq!(shard_shift(8), 5);
        assert_eq!(shard_shift(9), 6);
        assert_eq!(shard_shift(10), 6);
        assert_eq!(shard_shift(usize::MAX), usize::BITS - 1);
    }

    #[test]
    fn test_capacity_per_shard() {
        assert_eq!(capacity_per_shard(0, 2), 0);
        assert_eq!(capacity_per_shard(1, 2), 1);
        assert_eq!(capacity_per_shard(2, 2), 1);
        assert_eq!(capacity_per_shard(3, 2), 1);
        assert_eq!(capacity_per_shard(4, 2), 1);
        assert_eq!(capacity_per_shard(5, 2), 2);
        assert_eq!(capacity_per_shard(42, 2), 11);
        assert_eq!(capacity_per_shard(usize::MAX, 2), 1 << (usize::BITS - 2));
    }

    const NUM_READERS: usize = 4;
    const NUM_WRITERS: usize = 4;
    #[cfg(not(miri))]
    const NUM_ITEMS: usize = 100_000;
    #[cfg(miri)]
    const NUM_ITEMS: usize = 100;

    #[test]
    fn test_insert_unique() {
        let mut table = DashTable::default();
        let hasher = RandomState::default();

        thread::scope(|s| {
            let table = &table;
            let hasher = &hasher;
            for i in 0..NUM_WRITERS {
                s.spawn(move || {
                    for j in 0..NUM_ITEMS {
                        let value = i * NUM_ITEMS + j;
                        assert_eq!(
                            *table
                                .insert_unique(hasher.hash_one(value), value, |&x| {
                                    hasher.hash_one(x)
                                })
                                .get(),
                            value,
                        );
                    }
                });
            }
        });

        for value in 0..NUM_WRITERS * NUM_ITEMS {
            assert_eq!(
                table.find_mut(hasher.hash_one(value), |&x| x == value),
                Some(&value)
            );
        }
    }

    #[test]
    fn test_with_capacity_insert_unique() {
        let mut table = DashTable::with_capacity(NUM_WRITERS * NUM_ITEMS);
        let hasher = RandomState::default();

        thread::scope(|s| {
            let table = &table;
            let hasher = &hasher;
            for i in 0..NUM_WRITERS {
                s.spawn(move || {
                    for j in 0..NUM_ITEMS {
                        let value = i * NUM_ITEMS + j;
                        assert_eq!(
                            *table
                                .insert_unique(hasher.hash_one(value), value, |&x| {
                                    hasher.hash_one(x)
                                })
                                .get(),
                            value,
                        );
                    }
                });
            }
        });

        for value in 0..NUM_WRITERS * NUM_ITEMS {
            assert_eq!(
                table.find_mut(hasher.hash_one(value), |&x| x == value),
                Some(&value)
            );
        }
    }

    #[test]
    fn test_entry_or_insert_with() {
        let mut table: DashTable<(usize, usize)> = DashTable::default();
        let hasher = RandomState::default();

        thread::scope(|s| {
            let table = &table;
            let hasher = &hasher;
            for _ in 0..NUM_WRITERS {
                s.spawn(move || {
                    for value in 0..NUM_ITEMS {
                        table
                            .entry(
                                hasher.hash_one(value),
                                |x| x.0 == value,
                                |x| hasher.hash_one(x.0),
                            )
                            .or_insert_with(|| (value, 0))
                            .get_mut()
                            .1 += 1;
                    }
                });
            }
        });

        for value in 0..NUM_ITEMS {
            assert_eq!(
                table.find_mut(hasher.hash_one(value), |x| x.0 == value),
                Some(&(value, NUM_WRITERS))
            );
        }
    }

    #[test]
    fn test_entry_remove() {
        let mut table = DashTable::default();
        let hasher = RandomState::default();

        for value in 0..NUM_ITEMS {
            assert_eq!(
                *table
                    .insert_unique_mut(hasher.hash_one(value), value, |&x| { hasher.hash_one(x) })
                    .get(),
                value
            );
        }

        thread::scope(|s| {
            let table = &table;
            let hasher = &hasher;
            for _ in 0..NUM_WRITERS {
                s.spawn(move || {
                    for value in 0..NUM_ITEMS {
                        if let Entry::Occupied(entry) = table.entry(
                            hasher.hash_one(value),
                            |&x| x == value,
                            |&x| hasher.hash_one(x),
                        ) {
                            assert_eq!(entry.remove().0, value);
                        }
                    }
                });
            }
        });

        for value in 0..NUM_ITEMS {
            assert_eq!(
                table.find_mut(hasher.hash_one(value), |&x| x == value),
                None
            );
        }
    }

    #[test]
    fn test_entry_mut() {
        let mut table: DashTable<(usize, usize)> = DashTable::default();
        let hasher = RandomState::default();

        for _ in 0..NUM_WRITERS {
            for value in 0..NUM_ITEMS {
                table
                    .entry_mut(
                        hasher.hash_one(value),
                        |x| x.0 == value,
                        |x| hasher.hash_one(x.0),
                    )
                    .or_insert_with(|| (value, 0))
                    .get_mut()
                    .1 += 1;
            }
        }

        for value in 0..NUM_ITEMS {
            assert_eq!(
                table.find_mut(hasher.hash_one(value), |x| x.0 == value),
                Some(&(value, NUM_WRITERS))
            );
        }
    }

    #[test]
    fn test_find() {
        let mut table = DashTable::default();
        let hasher = RandomState::default();

        for value in 0..NUM_ITEMS {
            assert_eq!(
                *table
                    .insert_unique_mut(hasher.hash_one(value), value, |&x| { hasher.hash_one(x) })
                    .get(),
                value
            );
        }

        thread::scope(|s| {
            let table = &table;
            let hasher = &hasher;
            for _ in 0..NUM_READERS {
                s.spawn(move || {
                    for value in 0..NUM_ITEMS {
                        assert_eq!(
                            table
                                .find(hasher.hash_one(value), |&x| x == value)
                                .as_deref(),
                            Some(&value)
                        );
                    }
                });
            }
        });
    }

    #[test]
    fn test_concurrent_insert_unique_find() {
        let mut table: DashTable<usize> = DashTable::default();
        let hasher = RandomState::default();

        thread::scope(|s| {
            let table = &table;
            let hasher = &hasher;
            for _ in 0..NUM_READERS {
                s.spawn(move || {
                    'outer: loop {
                        for value in 0..NUM_WRITERS * NUM_ITEMS {
                            match table.find(hasher.hash_one(value), |&x| x == value) {
                                None => continue 'outer,
                                Some(v) => assert_eq!(*v.deref(), value),
                            }
                        }
                        break;
                    }
                });
            }
            for i in 0..NUM_WRITERS {
                s.spawn(move || {
                    for j in 0..NUM_ITEMS {
                        let value = i * NUM_ITEMS + j;
                        assert_eq!(
                            *table
                                .insert_unique(hasher.hash_one(value), value, |&x| {
                                    hasher.hash_one(x)
                                })
                                .get(),
                            value,
                        );
                    }
                });
            }
        });

        for value in 0..NUM_WRITERS * NUM_ITEMS {
            assert_eq!(
                table.find_mut(hasher.hash_one(value), |&x| x == value),
                Some(&value)
            );
        }
    }
}
