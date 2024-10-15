//! Space-efficient maps, designed for primarily immutable access. These maps are stored as sorted arrays and looked up via binary search, so that elements can be as packed together as possible to minimize space usage

use alloc::vec::{self, Vec};
use core::borrow::Borrow;
use core::fmt::{self, Debug, Formatter};
use core::mem;

/// A map from keys to values, implemented as a sorted array
///
/// The map is intended to be formed once and then only viewed, not modified actively, so that this format is efficient.
#[derive(Clone)]
pub struct Map<K: Ord, V> {
    /// The contents of this map, sorted by key
    contents: Vec<(K, V)>,
}

impl<K: Ord, V> Map<K, V> {
    /// Creates a new, empty `Map`. Does not allocate until used
    pub const fn new() -> Self {
        Self {
            contents: Vec::new(),
        }
    }

    /// Applies a slice's binary search to the given key and returns the relevant index.
    /// See those methods for more details on the return values
    fn search_for<Q: Ord + ?Sized>(&self, key: &Q) -> Result<usize, usize>
    where
        K: Borrow<Q>,
    {
        self.contents
            .binary_search_by_key(&key, |&(ref elem_key, _)| elem_key.borrow())
    }

    /// Inserts a key-value pair into the map.
    ///
    /// If the map did not have this key present, `None` is returned.
    ///
    /// If the map did have this key present, the value is updated, and the old value is returned.
    pub(crate) fn insert(&mut self, key: K, value: V) -> Option<V> {
        match self.search_for(&key) {
            Ok(index) => Some(mem::replace(
                #[expect(clippy::indexing_slicing, reason = "The indexing should never fail")]
                &mut self.contents[index].1,
                value,
            )),
            Err(index) => {
                self.contents.insert(index, (key, value));
                None
            }
        }
    }

    /// Removes a key from the map, returning the value at the key if the key was previously in the map
    pub(crate) fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        self.search_for(key.borrow())
            .ok()
            .map(|index| self.contents.remove(index).1)
    }

    /// Returns a reference to the value corresponding to the key.
    pub fn get<Q: Ord + ?Sized>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
    {
        self.search_for(key)
            .map(|index| {
                #[expect(clippy::indexing_slicing, reason = "The indexing should never fail")]
                &self.contents[index].1
            })
            .ok()
    }

    /// An iterator visiting all key-value pairs in sorted order by key
    pub fn iter(&self) -> impl Iterator<Item = &(K, V)> {
        self.contents.iter()
    }

    /// Creates an iterator which uses a closure to determine if an entry should be removed.
    ///
    /// If the closure returns true, then the entry is removed and yielded. If the closure returns false, the entry will remain in the map and will not be yielded by the iterator.
    pub fn extract_if<'map, F: FnMut(&K, &V) -> bool + 'map>(
        &'map mut self,
        mut filter: F,
    ) -> impl Iterator<Item = (K, V)> + 'map {
        self.contents
            .extract_if(move |&mut (ref key, ref value)| filter(key, value))
    }

    /// Returns `true` if the map contains no entries
    pub fn is_empty(&self) -> bool {
        self.contents.is_empty()
    }
}

impl<K: Ord + Clone, V: Clone> Map<K, V> {
    /// Extends the contents of this map with another map, but *maintains the current values* in the map instead of replacing duplicate keys
    pub(crate) fn extend_preserve(&mut self, other: &Self) {
        for &(ref key, ref value) in &other.contents {
            if let Err(index) = self.search_for(key) {
                self.contents.insert(index, (key.clone(), value.clone()));
            }
        }
    }
}

impl<K: Ord + Debug, V: Debug> Debug for Map<K, V> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_map()
            .entries(self.iter().map(|&(ref key, ref value)| (key, value)))
            .finish()
    }
}

impl<K: Ord, V> IntoIterator for Map<K, V> {
    type Item = (K, V);
    type IntoIter = vec::IntoIter<(K, V)>;

    fn into_iter(self) -> Self::IntoIter {
        self.contents.into_iter()
    }
}

impl<K: Ord, V> FromIterator<(K, V)> for Map<K, V> {
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        let mut contents: Vec<_> = iter.into_iter().collect();
        contents.sort_unstable_by(|&(ref key1, _), &(ref key2, _)| key1.cmp(key2));
        Self { contents }
    }
}

impl<K: Ord, V> Default for Map<K, V> {
    fn default() -> Self {
        Self {
            contents: Vec::default(),
        }
    }
}

impl<K: Ord, V> Extend<(K, V)> for Map<K, V> {
    fn extend<T: IntoIterator<Item = (K, V)>>(&mut self, iter: T) {
        for (key, value) in iter {
            self.insert(key, value);
        }
    }
}
