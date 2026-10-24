//! Shared arena roots: even a small inline persistent vector clones in O(1).
use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
};

#[derive(Clone, Debug)]
pub struct SharedVector<T: Clone>(Arc<im::Vector<T>>);

impl<T: Clone> SharedVector<T> {
    pub(crate) fn append_shared(&mut self, other: &Self) {
        if self.is_empty() {
            *self = other.clone();
        } else if !other.is_empty() {
            Arc::make_mut(&mut self.0).append(other.0.as_ref().clone());
        }
    }
    pub fn first(&self) -> Option<&T> {
        self.front()
    }
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl<T: Clone + PartialEq> PartialEq for SharedVector<T> {
    fn eq(&self, other: &Self) -> bool {
        self.ptr_eq(other) || self.0 == other.0
    }
}
impl<T: Clone> Deref for SharedVector<T> {
    type Target = im::Vector<T>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl<T: Clone> DerefMut for SharedVector<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        Arc::make_mut(&mut self.0)
    }
}
impl<T: Clone> From<Vec<T>> for SharedVector<T> {
    fn from(values: Vec<T>) -> Self {
        Self(Arc::new(values.into()))
    }
}
impl<T: Clone> FromIterator<T> for SharedVector<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Self(Arc::new(iter.into_iter().collect()))
    }
}
impl<T: Clone> IntoIterator for SharedVector<T> {
    type Item = T;
    type IntoIter = <im::Vector<T> as IntoIterator>::IntoIter;
    fn into_iter(self) -> Self::IntoIter {
        Arc::try_unwrap(self.0)
            .unwrap_or_else(|v| (*v).clone())
            .into_iter()
    }
}
impl<'a, T: Clone + 'a> IntoIterator for &'a SharedVector<T> {
    type Item = &'a T;
    type IntoIter = Box<dyn Iterator<Item = &'a T> + 'a>;
    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.iter())
    }
}

impl<T: Clone> Default for SharedVector<T> {
    fn default() -> Self {
        Self(Arc::new(im::Vector::new()))
    }
}

#[derive(Clone, Debug)]
struct Identity<T>(Arc<T>);
impl<T> PartialEq for Identity<T> {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// Persistent ordered map with shared values: copying a search path never clones
/// an unrelated AST, node, slot vector or diagnostic payload.
#[derive(Clone, Debug)]
pub struct SharedMap<K: Ord + Clone, V: Clone>(Arc<im::OrdMap<K, Identity<V>>>);
impl<K: Ord + Clone, V: Clone> Default for SharedMap<K, V> {
    fn default() -> Self {
        Self(Arc::new(im::OrdMap::new()))
    }
}
impl<K: Ord + Clone, V: Clone> SharedMap<K, V> {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    pub fn changed_keys(&self, other: &Self) -> Vec<K> {
        use im::ordmap::DiffItem;
        self.0
            .diff(&other.0)
            .map(|item| match item {
                DiffItem::Add(key, _) | DiffItem::Remove(key, _) => key.clone(),
                DiffItem::Update { new: (key, _), .. } => key.clone(),
            })
            .collect()
    }
    pub fn get<Q: Ord + ?Sized>(&self, key: &Q) -> Option<&V>
    where
        K: std::borrow::Borrow<Q>,
    {
        self.0.get(key).map(|v| v.0.as_ref())
    }
    pub fn contains_key<Q: Ord + ?Sized>(&self, key: &Q) -> bool
    where
        K: std::borrow::Borrow<Q>,
    {
        self.0.contains_key(key)
    }
    pub fn get_mut<Q: Ord + ?Sized>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: std::borrow::Borrow<Q>,
    {
        Arc::make_mut(&mut self.0)
            .get_mut(key)
            .map(|v| Arc::make_mut(&mut v.0))
    }
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        Arc::make_mut(&mut self.0)
            .insert(key, Identity(Arc::new(value)))
            .map(|v| (*v.0).clone())
    }
    pub fn remove<Q: Ord + ?Sized>(&mut self, key: &Q) -> Option<V>
    where
        K: std::borrow::Borrow<Q>,
    {
        Arc::make_mut(&mut self.0)
            .remove(key)
            .map(|v| (*v.0).clone())
    }
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = (&K, &V)> {
        self.0.iter().map(|(k, v)| (k, v.0.as_ref()))
    }
    pub(crate) fn range(
        &self,
        bounds: (std::ops::Bound<K>, std::ops::Bound<K>),
    ) -> impl DoubleEndedIterator<Item = (&K, &V)> {
        self.0
            .range(bounds)
            .map(|(key, value)| (key, value.0.as_ref()))
    }
    pub(crate) fn get_shared<Q: Ord + ?Sized>(&self, key: &Q) -> Option<Arc<V>>
    where
        K: std::borrow::Borrow<Q>,
    {
        self.0.get(key).map(|value| value.0.clone())
    }
    pub fn keys(&self) -> impl DoubleEndedIterator<Item = &K> {
        self.0.keys()
    }
    pub fn values(&self) -> impl DoubleEndedIterator<Item = &V> {
        self.0.values().map(|v| v.0.as_ref())
    }
}
impl<K: Ord + Clone, V: Clone + PartialEq> PartialEq for SharedMap<K, V> {
    fn eq(&self, other: &Self) -> bool {
        self.ptr_eq(other)
            || (self.len() == other.len()
                && self
                    .changed_keys(other)
                    .iter()
                    .all(|key| self.get(key) == other.get(key)))
    }
}
impl<K: Ord + Clone, V: Clone> FromIterator<(K, V)> for SharedMap<K, V> {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        Self(Arc::new(
            iter.into_iter()
                .map(|(k, v)| (k, Identity(Arc::new(v))))
                .collect(),
        ))
    }
}
impl<'a, K: Ord + Clone + 'a, V: Clone + 'a> IntoIterator for &'a SharedMap<K, V> {
    type Item = (&'a K, &'a V);
    type IntoIter = Box<dyn DoubleEndedIterator<Item = Self::Item> + 'a>;
    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.iter())
    }
}
impl<K: Ord + Clone, V: Clone> IntoIterator for SharedMap<K, V> {
    type Item = (K, V);
    type IntoIter = std::vec::IntoIter<(K, V)>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect::<Vec<_>>()
            .into_iter()
    }
}
impl<K: Ord + Clone, V: Clone, Q: Ord + ?Sized> std::ops::Index<&Q> for SharedMap<K, V>
where
    K: std::borrow::Borrow<Q>,
{
    type Output = V;
    fn index(&self, key: &Q) -> &V {
        self.0.index(key).0.as_ref()
    }
}

impl<'a, T: Clone + 'a> IntoIterator for &'a mut SharedVector<T> {
    type Item = &'a mut T;
    type IntoIter = Box<dyn Iterator<Item = &'a mut T> + 'a>;
    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.iter_mut())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    #[derive(Debug)]
    struct Payload(std::rc::Rc<Cell<usize>>, usize);
    impl Clone for Payload {
        fn clone(&self) -> Self {
            self.0.set(self.0.get() + 1);
            Self(self.0.clone(), self.1)
        }
    }
    #[test]
    fn unrelated_map_insertions_do_not_clone_payloads() {
        let clones = std::rc::Rc::new(Cell::new(0));
        let map: SharedMap<_, _> = (0..1000).map(|i| (i, Payload(clones.clone(), i))).collect();
        let mut changed = map.clone();
        changed.insert(1001, Payload(clones.clone(), 1001));
        assert_eq!(clones.get(), 0);
        assert_eq!(map.changed_keys(&changed), vec![1001]);
        assert_eq!(clones.get(), 0);
        changed.get_mut(&7).unwrap().1 = 42;
        assert_eq!(clones.get(), 1);
        assert_eq!(map.get(&7).unwrap().1, 7);
        assert_eq!(changed.get(&7).unwrap().1, 42);
        assert_eq!(map.changed_keys(&changed), vec![7, 1001]);
    }
    #[test]
    fn map_pointer_identity_distinguishes_equal_values_with_distinct_roots() {
        let a: SharedMap<u8, u8> = [(1, 2)].into_iter().collect();
        let clone = a.clone();
        assert!(a.ptr_eq(&clone));
        let b: SharedMap<u8, u8> = [(1, 2)].into_iter().collect();
        assert_eq!(a, b);
        assert!(!a.ptr_eq(&b), "equal contents must not report shared roots");
        let different_value: SharedMap<u8, u8> = [(1, 3)].into_iter().collect();
        assert_ne!(a, different_value);
        let different_key: SharedMap<u8, u8> = [(2, 2)].into_iter().collect();
        assert_ne!(a, different_key);
        let different_len: SharedMap<u8, u8> = [(1, 2), (2, 3)].into_iter().collect();
        assert_ne!(a, different_len);
    }
    #[test]
    fn small_arena_clones_share_the_root_and_preserve_old_values() {
        let values: SharedVector<_> = vec![String::from("a")].into();
        let mut next = values.clone();
        assert!(values.ptr_eq(&next));
        next.push_back(String::from("b"));
        assert!(!values.ptr_eq(&next));
        assert_eq!(
            values.iter().map(String::as_str).collect::<Vec<_>>(),
            vec!["a"]
        );
        assert_eq!(next.len(), 2);
    }
}
