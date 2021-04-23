use std::default::Default;
use std::marker::PhantomData;
use std::collections::VecDeque;

use indexmap::IndexMap;
use linked_hash_map::LinkedHashMap;
use twox_hash::RandomXxh3HashBuilder64;

const INS: usize = 10;

#[bench]
fn bench_indexmap(b: &mut test::Bencher) {
    let hasher = RandomXxh3HashBuilder64::default();
    let mut m = IndexMap::with_hasher(hasher);
    let mut k = Key::<32>::new();
    b.iter(move || {
        let mut i = 0;
        let k = loop {
            let k = k.next().unwrap();
            m.insert(k.clone(), ());
            if i == INS {
                break k;
            }
            i += 1;
        };
        m.pop();
        m.remove(&k);
    });
}

#[bench]
fn bench_linkedhashmap(b: &mut test::Bencher) {
    let hasher = RandomXxh3HashBuilder64::default();
    let mut m = LinkedHashMap::with_hasher(hasher);
    let mut k = Key::<32>::new();
    b.iter(move || {
        let mut i = 0;
        let k = loop {
            let k = k.next().unwrap();
            m.insert(k.clone(), ());
            if i == INS {
                break k;
            }
            i += 1;
        };
        m.pop_front();
        m.remove(&k);
    });
}

#[bench]
fn bench_vecdequemap(b: &mut test::Bencher) {
    let mut m = VecDequeMap::new();
    let mut k = Key::<32>::new();
    b.iter(move || {
        let mut i = 0;
        let k = loop {
            let k = k.next().unwrap();
            m.insert(k.clone(), ());
            if i == INS {
                break k;
            }
            i += 1;
        };
        m.pop_front();
        m.remove(&k);
    });
}

#[bench]
fn bench_vecdequemap_unique(b: &mut test::Bencher) {
    let mut m = VecDequeMap::new();
    let mut k = Key::<32>::new();
    b.iter(move || {
        let mut i = 0;
        let k = loop {
            let k = k.next().unwrap();
            unsafe { m.insert_unique(k.clone(), ()); }
            if i == INS {
                break k;
            }
            i += 1;
        };
        m.pop_front();
        m.remove(&k);
    });
}

struct Key<const N: usize> {
    x: u8,
    _marker: PhantomData<[u8; N]>,
}

impl<const N: usize> Key<N> {
    fn new() -> Self {
        Self { x: 0, _marker: PhantomData }
    }
}

impl<const N: usize> Iterator for Key<N> {
    type Item = [u8; N];

    fn next(&mut self) -> Option<[u8; N]> {
        let k = [self.x; N];
        self.x = self.x.wrapping_add(1);
        Some(k)
    }
}

struct VecDequeMap<K, V> {
    keys: VecDeque<K>,
    values: VecDeque<V>,
}

impl<K, V> VecDequeMap<K, V> {
    fn new() -> Self {
        Self {
            keys: VecDeque::new(),
            values: VecDeque::new(),
        }
    }

    fn insert(&mut self, k: K, mut v: V) -> Option<V>
    where
        K: Eq,
    {
        if let Some(i) = self.locate(&k) {
            std::mem::swap(&mut self.values[i], &mut v);
            return Some(v);
        }
        // key not present
        unsafe { self.insert_unique(k, v); }
        None
    }

    unsafe fn insert_unique(&mut self, k: K, v: V) {
        self.keys.push_back(k);
        self.values.push_back(v);
    }

    fn pop_front(&mut self) -> Option<(K, V)> {
        let k = self.keys.pop_front()?;
        let v = match self.values.pop_front() {
            Some(v) => v,
            None => unreachable!(),
        };
        Some((k, v))
    }

    fn remove(&mut self, k: &K) -> Option<V>
    where
        K: Eq,
    {
        let index = self.locate(k)?;
        self.keys.remove(index);
        let v = match self.values.remove(index) {
            Some(v) => v,
            None => unreachable!(),
        };
        Some(v)
    }

    fn locate(&mut self, k: &K) -> Option<usize>
    where
        K: Eq,
    {
        for (i, this_k) in self.keys.iter().enumerate() {
            if this_k == k {
                return Some(i);
            }
        }
        None
    }
}
