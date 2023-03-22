use std::{
    collections::{btree_map::Iter, BTreeMap, HashMap},
    iter::Rev,
    marker::PhantomData,
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
};

use serde::{de::DeserializeOwned, Serialize};

use crate::DatabaseManager;

use super::{DatabaseCollection, Error};

#[derive(Eq, PartialEq, Hash)]
struct EntryKey {
    string_section: String,
    additional_number: Option<u64>,
}

impl EntryKey {
    pub fn to_string(&self) -> String {
        match self.additional_number {
            Some(number) => format!("{}{}", self.string_section, number),
            None => self.string_section.clone(),
        }
    }
}

impl PartialOrd for EntryKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match self.string_section.partial_cmp(&other.string_section) {
            Some(core::cmp::Ordering::Equal) => {}
            ord => return ord,
        }
        self.additional_number.partial_cmp(&other.additional_number)
    }
}

impl Ord for EntryKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.string_section.cmp(&other.string_section) {
            core::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        self.additional_number.cmp(&other.additional_number)
    }
}

pub struct DataStore<K: Ord + PartialOrd, V> {
    data: RwLock<BTreeMap<K, V>>,
}

impl<K: Ord + PartialOrd, V> DataStore<K, V> {
    fn new() -> Self {
        Self {
            data: RwLock::new(BTreeMap::new()),
        }
    }

    fn _get_inner_read_lock<'a>(&'a self) -> RwLockReadGuard<'a, BTreeMap<K, V>> {
        self.data.read().unwrap()
    }

    fn _get_inner_write_lock<'a>(&'a self) -> RwLockWriteGuard<'a, BTreeMap<K, V>> {
        self.data.write().unwrap()
    }
}

impl DataStore<EntryKey, Vec<u8>> {
    fn iter<V: Serialize + DeserializeOwned>(&self, entry_prefix: String) -> MemoryIterator<V> {
        MemoryIterator::<V>::new(&self, entry_prefix)
    }

    fn rev_iter<V: Serialize + DeserializeOwned>(&self, entry_prefix: String) -> RevMemoryIterator<V> {
        RevMemoryIterator::<V>::new(&self, entry_prefix)
    }
}

pub struct MemoryManager {
    data: RwLock<HashMap<String, Arc<DataStore<EntryKey, Vec<u8>>>>>,
}

impl MemoryManager {
    pub fn new() -> Self {
        Self {
            data: RwLock::new(HashMap::new()),
        }
    }
}

impl DatabaseManager for MemoryManager {
    fn create_collection<V>(
        &self,
        identifier: &str,
    ) -> Box<dyn super::DatabaseCollection<InnerDataType = V>>
    where
        V: serde::Serialize + serde::de::DeserializeOwned + Sync + Send + 'static,
    {
        let id = identifier.to_owned();
        let mut lock = self.data.write().unwrap();
        let db = match lock.get(&id) {
            Some(map) => map.clone(),
            None => {
                let db = Arc::new(DataStore::new());
                lock.insert(identifier.to_owned(), db.clone());
                db
            }
        };
        Box::new(MemoryCollection {
            level: identifier.to_owned(),
            separator: char::MAX,
            data: db,
            _v: PhantomData::default(),
        })
    }
}

pub struct MemoryCollection<V: Serialize + DeserializeOwned> {
    level: String,
    separator: char,
    data: Arc<DataStore<EntryKey, Vec<u8>>>,
    _v: PhantomData<V>,
}

impl<V: Serialize + DeserializeOwned> MemoryCollection<V> {
    fn generate_key(&self, key: &str) -> EntryKey {
        match key.parse::<u64>() {
            Ok(data) => EntryKey {
                string_section: format!("{}{}", self.level, self.separator),
                additional_number: Some(data),
            },
            Err(_) => EntryKey {
                string_section: format!("{}{}{}", self.level, self.separator, key),
                additional_number: None,
            },
        }
    }
}

impl<V: Serialize + DeserializeOwned + Sync + Send> DatabaseCollection for MemoryCollection<V> {
    type InnerDataType = V;

    fn put(&self, key: &str, data: Self::InnerDataType) -> Result<(), super::Error> {
        let key = self.generate_key(key);
        let Ok(bytes) = bincode::serialize::<V>(&data) else {
            return Err(Error::SerializeError);
        };
        let mut lock = self.data._get_inner_write_lock();
        lock.insert(key, bytes);
        Ok(())
    }

    fn get(&self, key: &str) -> Result<Self::InnerDataType, super::Error> {
        let key = self.generate_key(key);
        let lock = self.data._get_inner_read_lock();
        let Some(data) = lock.get(&key) else {
            return Err(Error::EntryNotFound);
        };
        let Ok(result) = bincode::deserialize::<V>(data) else {
            return Err(Error::DeserializeError);
        };
        Ok(result)
    }

    fn del(&self, key: &str) -> Result<(), super::Error> {
        let key = self.generate_key(key);
        let mut lock = self.data._get_inner_write_lock();
        lock.remove(&key);
        Ok(())
    }

    fn partition<'a>(
        &'a self,
        key: &str,
    ) -> Box<dyn DatabaseCollection<InnerDataType = Self::InnerDataType> + 'a> {
        let new_level = format!("{}{}{}", self.level, self.separator, key);
        Box::new(Self {
            level: new_level,
            separator: self.separator.clone(),
            data: self.data.clone(),
            _v: PhantomData::default(),
        })
    }

    fn iter<'a>(&'a self) -> Box<dyn Iterator<Item = (String, Self::InnerDataType)> + 'a> {
        let iter = self.data.iter(format!("{}{}", self.level, self.separator));
        Box::new(iter)
    }

    fn rev_iter<'a>(&'a self) -> Box<dyn Iterator<Item = (String, Self::InnerDataType)> + 'a> {
        let iter = self.data.rev_iter(format!("{}{}", self.level, self.separator));
        Box::new(iter)
    }
}

type GuardIter<'a, K, V> = (Arc<RwLockReadGuard<'a, BTreeMap<K, V>>>, Iter<'a, K, V>);

pub struct MemoryIterator<'a, V> {
    map: &'a DataStore<EntryKey, Vec<u8>>,
    current: Option<GuardIter<'a, EntryKey, Vec<u8>>>,
    separator: String,
    partition_found: bool,
    _v: PhantomData<V>,
}

impl<'a, V> MemoryIterator<'a, V> {
    fn new(map: &'a DataStore<EntryKey, Vec<u8>>, entry_prefix: String) -> Self {
        Self {
            map,
            current: None,
            separator: entry_prefix,
            partition_found: false,
            _v: PhantomData::default(),
        }
    }
}

impl<'a, V: Serialize + DeserializeOwned> Iterator for MemoryIterator<'a, V> {
    type Item = (String, V);
    fn next(&mut self) -> Option<Self::Item> {
        let iter = if let Some((_, iter)) = self.current.as_mut() {
            iter
        } else {
            let guard = self.map._get_inner_read_lock();
            let sref: &BTreeMap<EntryKey, Vec<u8>> = unsafe { change_lifetime_const(&*guard) };
            let iter = sref.iter();
            self.current = Some((Arc::new(guard), iter));
            &mut self.current.as_mut().unwrap().1
        };
        loop {
            let Some((key, val)) = iter.next() else {
                return None;
            };
            let key = key.to_string();
            if key.starts_with(&self.separator) {
                self.partition_found = true;
                let key = key.to_string();
                let key = key.replace(&self.separator, "");
                let val: V = bincode::deserialize(val).unwrap();
                return Some((key.clone(), val));
            } else if self.partition_found {
                return None;
            }
        };
    }
}

type GuardRevIter<'a, K, V> = (
    Arc<RwLockReadGuard<'a, BTreeMap<K, V>>>,
    Rev<Iter<'a, K, V>>,
);

pub struct RevMemoryIterator<'a, V> {
    map: &'a DataStore<EntryKey, Vec<u8>>,
    current: Option<GuardRevIter<'a, EntryKey, Vec<u8>>>,
    separator: String,
    partition_found: bool,
    _v: PhantomData<V>,
}

impl<'a, V> RevMemoryIterator<'a, V> {
    fn new(map: &'a DataStore<EntryKey, Vec<u8>>, entry_prefix: String) -> Self {
        Self {
            map,
            current: None,
            separator: entry_prefix,
            partition_found: false,
            _v: PhantomData::default(),
        }
    }
}

impl<'a, V: Serialize + DeserializeOwned> Iterator for RevMemoryIterator<'a, V> {
    type Item = (String, V);
    fn next(&mut self) -> Option<Self::Item> {
        let iter = if let Some((_, iter)) = self.current.as_mut() {
            iter
        } else {
            let guard = self.map._get_inner_read_lock();
            let sref: &BTreeMap<EntryKey, Vec<u8>> = unsafe { change_lifetime_const(&*guard) };
            let iter = sref.iter().rev();
            self.current = Some((Arc::new(guard), iter));
            &mut self.current.as_mut().unwrap().1
        };
        loop {
            let Some((key, val)) = iter.next() else {
                return None;
            };
            let key = key.to_string();
            if key.starts_with(&self.separator) {
                self.partition_found = true;
                let key = key.to_string();
                let key = key.replace(&self.separator, "");
                let val: V = bincode::deserialize(val).unwrap();
                return Some((key.clone(), val));
            } else if self.partition_found {
                return None;
            }
        };
    }
}

unsafe fn change_lifetime_const<'a, 'b, T>(x: &'a T) -> &'b T {
    &*(x as *const T)
}

#[cfg(test)]
mod test {
    use serde::{Serialize, Deserialize};
    use crate::{database::DatabaseCollection, DatabaseManager};
    use super::MemoryManager;

    #[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
    struct Data {
        id: usize,
        value: String,
    }

    #[test]
    fn basic_operations_test() {
        let db = MemoryManager::new();
        let first_collection: Box<dyn DatabaseCollection<InnerDataType = Data>> =
            db.create_collection("first");
        // PUT & GET Operations
        // PUT
        let result = first_collection.put(
            "a",
            Data {
                id: 1,
                value: "A".into(),
            },
        );
        assert!(result.is_ok());
        let result = first_collection.put(
            "b",
            Data {
                id: 2,
                value: "B".into(),
            },
        );
        assert!(result.is_ok());
        let result = first_collection.put(
            "c",
            Data {
                id: 3,
                value: "C".into(),
            },
        );
        assert!(result.is_ok());
        // GET
        let result = first_collection.get("a");
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            Data {
                id: 1,
                value: "A".into()
            }
        );
        let result = first_collection.get("b");
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            Data {
                id: 2,
                value: "B".into()
            }
        );
        let result = first_collection.get("c");
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            Data {
                id: 3,
                value: "C".into()
            }
        );
        // DEL
        let result = first_collection.del("a");
        assert!(result.is_ok());
        let result = first_collection.del("b");
        assert!(result.is_ok());
        let result = first_collection.del("c");
        assert!(result.is_ok());
        // GET OF DELETED ENTRIES
        let result = first_collection.get("a");
        assert!(result.is_err());
        let result = first_collection.get("b");
        assert!(result.is_err());
        let result = first_collection.get("c");
        assert!(result.is_err());
    } 

    #[test]
    fn partitions_test() {
        let db = MemoryManager::new();
        let first_collection: Box<dyn DatabaseCollection<InnerDataType = Data>> =
            db.create_collection("first");
        let second_collection: Box<dyn DatabaseCollection<InnerDataType = Data>> =
            db.create_collection("second");
        // PUT UNIQUE ENTRIES IN EACH PARTITION
        let result = first_collection.put(
            "a",
            Data {
                id: 1,
                value: "A".into(),
            },
        );
        assert!(result.is_ok());
        let result = second_collection.put(
            "b",
            Data {
                id: 2,
                value: "B".into(),
            },
        );
        assert!(result.is_ok());
        // TRYING TO GET ENTRIES FROM A DIFFERENT PARTITION
        let result = first_collection.get("b");
        assert!(result.is_err());
        let result = second_collection.get("a");
        assert!(result.is_err());
    }

    #[test]
    fn inner_partition() {
        let db = MemoryManager::new();
        let first_collection: Box<dyn DatabaseCollection<InnerDataType = Data>> =
            db.create_collection("first");
        let inner_collection: Box<dyn DatabaseCollection<InnerDataType = Data>> =
            first_collection.partition("inner");
        // PUT OPERATIONS
        let result = first_collection.put(
            "a",
            Data {
                id: 1,
                value: "A".into(),
            },
        );
        assert!(result.is_ok());
        let result = inner_collection.put(
            "b",
            Data {
                id: 2,
                value: "B".into(),
            },
        );
        assert!(result.is_ok());
        // TRYING TO GET ENTRIES FROM A DIFFERENT PARTITION
        let result = first_collection.get("b");
        assert!(result.is_err());
        let result = inner_collection.get("a");
        assert!(result.is_err());
    }

    fn build_state(collection: &Box<dyn DatabaseCollection<InnerDataType = Data>>) {
        let result = collection.put(
            "a",
            Data {
                id: 1,
                value: "A".into(),
            },
        );
        assert!(result.is_ok());
        let result = collection.put(
            "b",
            Data {
                id: 2,
                value: "B".into(),
            },
        );
        assert!(result.is_ok());
        let result = collection.put(
            "c",
            Data {
                id: 3,
                value: "C".into(),
            },
        );
        assert!(result.is_ok());
    }

    fn build_initial_data() -> (Vec<&'static str>, Vec<Data>) {
        let keys = vec!["a", "b", "c"];
        let data = vec![
            Data {
                id: 1,
                value: "A".into(),
            },
            Data {
                id: 2,
                value: "B".into(),
            },
            Data {
                id: 3,
                value: "C".into(),
            },
        ];
        (keys, data)
    }

    #[test]
    fn iterator_test() {
        let db = MemoryManager::new();
        let first_collection: Box<dyn DatabaseCollection<InnerDataType = Data>> =
            db.create_collection("first");
        build_state(&first_collection);
        // ITER TEST
        let mut iter = first_collection.iter();
        let (keys, data) = build_initial_data();
        for i in 0..3 {
            let (key, val) = iter.next().unwrap();
            assert_eq!(keys[i], key);
            assert_eq!(data[i], val);
        }
        assert!(iter.next().is_none());
    }

    #[test]
    fn rev_iterator_test() {
        let db = MemoryManager::new();
        let first_collection: Box<dyn DatabaseCollection<InnerDataType = Data>> =
            db.create_collection("first");
        build_state(&first_collection);
        // ITER TEST
        let mut iter = first_collection.rev_iter();
        let (keys, data) = build_initial_data();
        for i in (0..3).rev() {
            let (key, val) = iter.next().unwrap();
            assert_eq!(keys[i], key);
            assert_eq!(data[i], val);
        }
        assert!(iter.next().is_none());
    }


    #[test]
    fn iterator_with_various_collection_test() {
        let db = MemoryManager::new();
        let first_collection: Box<dyn DatabaseCollection<InnerDataType = Data>> =
            db.create_collection("first");
        let second_collection: Box<dyn DatabaseCollection<InnerDataType = Data>> =
            db.create_collection("second");
        build_state(&first_collection);
        let result = second_collection.put(
            "d",
            Data {
                id: 4,
                value: "D".into(),
            },
        );
        assert!(result.is_ok());
        let result = second_collection.put(
            "e",
            Data {
                id: 5,
                value: "E".into(),
            },
        );
        assert!(result.is_ok());
        let mut iter = first_collection.iter();
        let (keys, data) = build_initial_data();
        for i in 0..3 {
            let (key, val) = iter.next().unwrap();
            assert_eq!(keys[i], key);
            assert_eq!(data[i], val);
        }
        assert!(iter.next().is_none());

        let mut iter = second_collection.iter();
        let (keys, data) = (
            vec!["d", "e"],
            vec![
                Data {
                    id: 4,
                    value: "D".into(),
                },
                Data {
                    id: 5,
                    value: "E".into(),
                },
            ],
        );
        for i in 0..2 {
            let (key, val) = iter.next().unwrap();
            assert_eq!(keys[i], key);
            assert_eq!(data[i], val);
        }
        assert!(iter.next().is_none());
    }

    #[test]
    fn rev_iterator_with_various_collection_test() {
        let db = MemoryManager::new();
        let first_collection: Box<dyn DatabaseCollection<InnerDataType = Data>> =
            db.create_collection("first");
        let second_collection: Box<dyn DatabaseCollection<InnerDataType = Data>> =
            db.create_collection("second");
        build_state(&first_collection);
        let result = second_collection.put(
            "d",
            Data {
                id: 4,
                value: "D".into(),
            },
        );
        assert!(result.is_ok());
        let result = second_collection.put(
            "e",
            Data {
                id: 5,
                value: "E".into(),
            },
        );
        assert!(result.is_ok());
        let mut iter = first_collection.rev_iter();
        let (keys, data) = build_initial_data();
        for i in (0..3).rev() {
            let (key, val) = iter.next().unwrap();
            assert_eq!(keys[i], key);
            assert_eq!(data[i], val);
        }
        assert!(iter.next().is_none());

        let mut iter = second_collection.rev_iter();
        let (keys, data) = (
            vec!["d", "e"],
            vec![
                Data {
                    id: 4,
                    value: "D".into(),
                },
                Data {
                    id: 5,
                    value: "E".into(),
                },
            ],
        );
        for i in (0..2).rev() {
            let (key, val) = iter.next().unwrap();
            assert_eq!(keys[i], key);
            assert_eq!(data[i], val);
        }
        assert!(iter.next().is_none());
    }

    #[test]
    fn iteration_with_partitions_test() {
        let db = MemoryManager::new();
        let first_collection: Box<dyn DatabaseCollection<InnerDataType = Data>> =
            db.create_collection("first");
        let first_inner = first_collection.partition("inner1");
        let second_inner = first_inner.partition("inner2");
        first_collection.put("a", Data {
            id: 0,
            value: "A".into(),
        }).unwrap();
        first_inner.put("b", Data {
            id: 0,
            value: "B".into(),
        }).unwrap();
        second_inner.put("c", Data {
            id: 0,
            value: "C".into(),
        }).unwrap();
        let mut iter = second_inner.iter();
        let item = iter.next().unwrap();
        assert_eq!(&item.0, "c");
        assert!(iter.next().is_none());

        let mut iter = first_inner.iter();
        let item = iter.next().unwrap();
        assert_eq!(&item.0, "b");
        let item = iter.next().unwrap();
        assert_eq!(&item.0, "inner2\u{10ffff}c");
        assert!(iter.next().is_none());

        let mut iter = first_collection.iter();
        let item = iter.next().unwrap();
        assert_eq!(&item.0, "a");
        let item = iter.next().unwrap();
        assert_eq!(&item.0, "inner1\u{10ffff}b");
        let item = iter.next().unwrap();
        assert_eq!(&item.0, "inner1\u{10ffff}inner2\u{10ffff}c");
        assert!(iter.next().is_none());
    }


    #[test]
    fn rev_iteration_with_partitions_test() {
        let db = MemoryManager::new();
        let first_collection: Box<dyn DatabaseCollection<InnerDataType = Data>> =
            db.create_collection("first");
        let first_inner = first_collection.partition("inner1");
        let second_inner = first_inner.partition("inner2");
        first_collection.put("a", Data {
            id: 0,
            value: "A".into(),
        }).unwrap();
        first_inner.put("b", Data {
            id: 0,
            value: "B".into(),
        }).unwrap();
        second_inner.put("c", Data {
            id: 0,
            value: "C".into(),
        }).unwrap();
        let mut iter = second_inner.rev_iter();
        let item = iter.next().unwrap();
        assert_eq!(&item.0, "c");
        assert!(iter.next().is_none());

        let mut iter = first_inner.rev_iter();
        let item = iter.next().unwrap();
        assert_eq!(&item.0, "inner2\u{10ffff}c");
        let item = iter.next().unwrap();
        assert_eq!(&item.0, "b");
        assert!(iter.next().is_none());

        let mut iter = first_collection.rev_iter();
        let item = iter.next().unwrap();
        assert_eq!(&item.0, "inner1\u{10ffff}inner2\u{10ffff}c");
        let item = iter.next().unwrap();
        assert_eq!(&item.0, "inner1\u{10ffff}b");
        let item = iter.next().unwrap();
        assert_eq!(&item.0, "a");
        assert!(iter.next().is_none());
    }


}
