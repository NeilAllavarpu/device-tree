use core::borrow::Borrow;
use core::fmt::Debug;
use core::mem;
use core::ops::Deref;

use alloc::vec;

pub struct Map<K: Ord, V> {
    contents: Vec<(K, V)>,
}

impl<K: Ord + Debug, V> Map<K, V> {
    pub const fn new() -> Self {
        Self {
            contents: Vec::new(),
        }
    }

    fn search_for<Q: Ord + ?Sized>(&self, key: &Q) -> Result<usize, usize>
    where
        K: Borrow<Q>,
    {
        self.contents.binary_search_by(|(k, v)| k.borrow().cmp(key))
    }

    pub(crate) fn insert(&mut self, key: K, value: V) -> Option<V> {
        match self.search_for(&key) {
            Ok(index) => Some(mem::replace(&mut self.contents[index].1, value)),
            Err(index) => {
                self.contents.insert(index, (key, value));
                None
            }
        }
    }

    pub(crate) fn remove<'a, 'b, Q: Ord + ?Sized>(&'a mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
    {
        self.search_for(key.borrow())
            .ok()
            .map(|index| self.contents.remove(index).1)
    }

    pub fn get<Q: Ord + ?Sized>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
    {
        self.search_for(key)
            .map(|index| &self.contents[index].1)
            .ok()
    }

    pub fn iter(&self) -> impl Iterator<Item = &(K, V)> {
        self.contents.iter()
    }

    pub fn extract_if(
        &mut self,
        mut filter: impl FnMut(&K, &V) -> bool + 'static,
    ) -> impl Iterator<Item = (K, V)> + '_ {
        self.contents.extract_if(move |(k, v)| filter(k, v))
    }

    pub fn is_empty(&self) -> bool {
        self.contents.is_empty()
    }

    pub fn p_keys(&self) {
        for elem in &self.contents {
            println!("{:?}", elem.0);
        }
    }
}

impl<K: Ord + Clone, V: Clone> Map<K, V> {
    pub fn merge_preserve(&mut self, other: &Self) {
        for (key, value) in &other.contents {
            if let Err(index) = self.contents.binary_search_by_key(&key, |(k, _)| k) {
                self.contents.insert(index, (key.clone(), value.clone()));
            }
        }
    }
}

impl<K: Ord + Debug, V: Debug> Debug for Map<K, V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut m = f.debug_map();
        for (k, v) in self.iter() {
            m.entry(k, v);
        }
        m.finish()
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
        contents.sort_unstable_by(|(key1, _), (key2, _)| key1.cmp(key2));
        Self { contents }
    }
}
