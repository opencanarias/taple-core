use std::{
    collections::{btree_map::Iter, BTreeMap, HashMap},
    iter::Rev,
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
};

use crate::DatabaseManager;

use super::{DatabaseCollection, Error};

pub struct DataStore {
    data: RwLock<BTreeMap<String, Vec<u8>>>,
}

impl DataStore {
    fn new() -> Self {
        Self {
            data: RwLock::new(BTreeMap::new()),
        }
    }

    fn _get_inner_read_lock<'a>(&'a self) -> RwLockReadGuard<'a, BTreeMap<String, Vec<u8>>> {
        self.data.read().unwrap()
    }

    fn _get_inner_write_lock<'a>(&'a self) -> RwLockWriteGuard<'a, BTreeMap<String, Vec<u8>>> {
        self.data.write().unwrap()
    }
}

impl DataStore {
    fn iter(&self, entry_prefix: String) -> MemoryIterator {
        MemoryIterator::new(&self, entry_prefix)
    }

    fn rev_iter(&self, entry_prefix: String) -> RevMemoryIterator {
        RevMemoryIterator::new(&self, entry_prefix)
    }
}

pub struct MemoryManager {
    data: RwLock<HashMap<String, Arc<DataStore>>>,
}

impl MemoryManager {
    pub fn new() -> Self {
        Self {
            data: RwLock::new(HashMap::new()),
        }
    }
}

impl DatabaseManager<MemoryCollection> for MemoryManager {
    fn create_collection(&self, identifier: &str) -> MemoryCollection {
        let mut lock = self.data.write().unwrap();
        let db = match lock.get(identifier) {
            Some(map) => map.clone(),
            None => {
                let db = Arc::new(DataStore::new());
                db
            }
        };
        MemoryCollection { data: db }
    }
}

pub struct MemoryCollection {
    data: Arc<DataStore>,
}

impl DatabaseCollection for MemoryCollection {
    fn get(&self, key: &str) -> Result<Vec<u8>, super::Error> {
        let lock = self.data._get_inner_read_lock();
        let Some(data) = lock.get(key) else {
            return Err(Error::EntryNotFound);
        };
        Ok(data.clone())
    }

    fn put(&self, key: &str, data: Vec<u8>) -> Result<(), Error> {
        let mut lock = self.data._get_inner_write_lock();
        lock.insert(key.to_string(), data);
        Ok(())
    }

    fn del(&self, key: &str) -> Result<(), Error> {
        let mut lock = self.data._get_inner_write_lock();
        lock.remove(key);
        Ok(())
    }

    fn iter<'a>(
        &'a self,
        reverse: bool,
        prefix: String,
    ) -> Box<dyn Iterator<Item = (String, Vec<u8>)> + 'a> {
        if reverse {
            Box::new(self.data.rev_iter(format!("{}", prefix)))
        } else {
            Box::new(self.data.iter(format!("{}", prefix)))
        }
    }
}

type GuardIter<'a, K, V> = (Arc<RwLockReadGuard<'a, BTreeMap<K, V>>>, Iter<'a, K, V>);

pub struct MemoryIterator<'a> {
    map: &'a DataStore,
    current: Option<GuardIter<'a, String, Vec<u8>>>,
    prefix: String,
    key: String,
    value: Vec<u8>,
}

impl<'a> MemoryIterator<'a> {
    fn new(map: &'a DataStore, entry_prefix: String) -> Self {
        Self {
            map,
            current: None,
            prefix: entry_prefix,
            key: "".to_string(),
            value: vec![],
        }
    }
}

impl<'a> Iterator for MemoryIterator<'a> {
    type Item = (String, Vec<u8>);
    fn next(&mut self) -> Option<Self::Item> {
        let iter = if let Some((_, iter)) = self.current.as_mut() {
            iter
        } else {
            let guard = self.map._get_inner_read_lock();
            let sref: &BTreeMap<String, Vec<u8>> = unsafe { change_lifetime_const(&*guard) };
            let iter = sref.iter();
            self.current = Some((Arc::new(guard), iter));
            &mut self.current.as_mut().unwrap().1
        };
        loop {
            let Some((key, val)) = iter.next() else {
                return None;
            };
            let key = key.to_string();
            if key.starts_with(&self.prefix) {
                let key = key.replace(&self.prefix, "");
                self.key = key.clone();
                self.value = val.clone();
                return Some((key.clone(), val.clone()));
            } else {
                return None;
            }
        }
    }
}

type GuardRevIter<'a> = (
    Arc<RwLockReadGuard<'a, BTreeMap<String, Vec<u8>>>>,
    Rev<Iter<'a, String, Vec<u8>>>,
);

pub struct RevMemoryIterator<'a> {
    map: &'a DataStore,
    current: Option<GuardRevIter<'a>>,
    prefix: String,
    key: String,
    value: Vec<u8>,
}

impl<'a> RevMemoryIterator<'a> {
    fn new(map: &'a DataStore, entry_prefix: String) -> Self {
        Self {
            map,
            current: None,
            prefix: entry_prefix,
            key: "".to_string(),
            value: vec![],
        }
    }
}

impl<'a> Iterator for RevMemoryIterator<'a> {
    type Item = (String, Vec<u8>);
    fn next(&mut self) -> Option<Self::Item> {
        let iter = if let Some((_, iter)) = self.current.as_mut() {
            iter
        } else {
            let guard = self.map._get_inner_read_lock();
            let sref: &BTreeMap<String, Vec<u8>> = unsafe { change_lifetime_const(&*guard) };
            let iter = sref.iter().rev();
            self.current = Some((Arc::new(guard), iter));
            &mut self.current.as_mut().unwrap().1
        };
        loop {
            let Some((key, val)) = iter.next() else {
                return None;
            };
            if key.starts_with(&self.prefix) {
                let key = key.replace(&self.prefix, "");
                return Some((key.clone(), val.clone()));
            } else {
                return None;
            }
        }
    }
}

unsafe fn change_lifetime_const<'a, 'b, T>(x: &'a T) -> &'b T {
    &*(x as *const T)
}

//#[cfg(test)]
//mod test {
//    use super::MemoryManager;
//    use crate::{database::DatabaseCollection, DatabaseManager};
//    use serde::{Deserialize, Serialize};
//
//    #[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
//    struct Data {
//        id: usize,
//        value: String,
//    }
//
//    #[test]
//    fn basic_operations_test() {
//        let db = MemoryManager::new();
//        let first_collection: Box<dyn DatabaseCollection<InnerDataType = Data>> =
//            db.create_collection("first");
//        // PUT & GET Operations
//        // PUT
//        let result = first_collection.put(
//            "a",
//            Data {
//                id: 1,
//                value: "A".into(),
//            },
//        );
//        assert!(result.is_ok());
//        let result = first_collection.put(
//            "b",
//            Data {
//                id: 2,
//                value: "B".into(),
//            },
//        );
//        assert!(result.is_ok());
//        let result = first_collection.put(
//            "c",
//            Data {
//                id: 3,
//                value: "C".into(),
//            },
//        );
//        assert!(result.is_ok());
//        // GET
//        let result = first_collection.get("a");
//        assert!(result.is_ok());
//        assert_eq!(
//            result.unwrap(),
//            Data {
//                id: 1,
//                value: "A".into()
//            }
//        );
//        let result = first_collection.get("b");
//        assert!(result.is_ok());
//        assert_eq!(
//            result.unwrap(),
//            Data {
//                id: 2,
//                value: "B".into()
//            }
//        );
//        let result = first_collection.get("c");
//        assert!(result.is_ok());
//        assert_eq!(
//            result.unwrap(),
//            Data {
//                id: 3,
//                value: "C".into()
//            }
//        );
//        // DEL
//        let result = first_collection.del("a");
//        assert!(result.is_ok());
//        let result = first_collection.del("b");
//        assert!(result.is_ok());
//        let result = first_collection.del("c");
//        assert!(result.is_ok());
//        // GET OF DELETED ENTRIES
//        let result = first_collection.get("a");
//        assert!(result.is_err());
//        let result = first_collection.get("b");
//        assert!(result.is_err());
//        let result = first_collection.get("c");
//        assert!(result.is_err());
//    }
//
//    #[test]
//    fn partitions_test() {
//        let db = MemoryManager::new();
//        let first_collection: Box<dyn DatabaseCollection<InnerDataType = Data>> =
//            db.create_collection("first");
//        let second_collection: Box<dyn DatabaseCollection<InnerDataType = Data>> =
//            db.create_collection("second");
//        // PUT UNIQUE ENTRIES IN EACH PARTITION
//        let result = first_collection.put(
//            "a",
//            Data {
//                id: 1,
//                value: "A".into(),
//            },
//        );
//        assert!(result.is_ok());
//        let result = second_collection.put(
//            "b",
//            Data {
//                id: 2,
//                value: "B".into(),
//            },
//        );
//        assert!(result.is_ok());
//        // TRYING TO GET ENTRIES FROM A DIFFERENT PARTITION
//        let result = first_collection.get("b");
//        assert!(result.is_err());
//        let result = second_collection.get("a");
//        assert!(result.is_err());
//    }
//
//    #[test]
//    fn inner_partition() {
//        let db = MemoryManager::new();
//        let first_collection: Box<dyn DatabaseCollection<InnerDataType = Data>> =
//            db.create_collection("first");
//        let inner_collection: Box<dyn DatabaseCollection<InnerDataType = Data>> =
//            first_collection.partition("inner");
//        // PUT OPERATIONS
//        let result = first_collection.put(
//            "a",
//            Data {
//                id: 1,
//                value: "A".into(),
//            },
//        );
//        assert!(result.is_ok());
//        let result = inner_collection.put(
//            "b",
//            Data {
//                id: 2,
//                value: "B".into(),
//            },
//        );
//        assert!(result.is_ok());
//        // TRYING TO GET ENTRIES FROM A DIFFERENT PARTITION
//        let result = first_collection.get("b");
//        assert!(result.is_err());
//        let result = inner_collection.get("a");
//        assert!(result.is_err());
//    }
//
//    fn build_state(collection: &Box<dyn DatabaseCollection>) {
//        let result = collection.put(
//            "a",
//            Data {
//                id: 1,
//                value: "A".into(),
//            },
//        );
//        assert!(result.is_ok());
//        let result = collection.put(
//            "b",
//            Data {
//                id: 2,
//                value: "B".into(),
//            },
//        );
//        assert!(result.is_ok());
//        let result = collection.put(
//            "c",
//            Data {
//                id: 3,
//                value: "C".into(),
//            },
//        );
//        assert!(result.is_ok());
//    }
//
//    fn build_initial_data() -> (Vec<&'static str>, Vec<Data>) {
//        let keys = vec!["a", "b", "c"];
//        let data = vec![
//            Data {
//                id: 1,
//                value: "A".into(),
//            },
//            Data {
//                id: 2,
//                value: "B".into(),
//            },
//            Data {
//                id: 3,
//                value: "C".into(),
//            },
//        ];
//        (keys, data)
//    }
//
//    #[test]
//    fn iterator_test() {
//        let db = MemoryManager::new();
//        let first_collection: Box<dyn DatabaseCollection> = db.create_collection("first");
//        build_state(&first_collection);
//        // ITER TEST
//        let mut iter = first_collection.iter(false);
//        let (keys, data) = build_initial_data();
//        for i in 0..3 {
//            let (key, val) = iter.next().unwrap();
//            assert_eq!(keys[i], key);
//            assert_eq!(data[i], val);
//        }
//        assert!(iter.next().is_none());
//    }
//
//    #[test]
//    fn rev_iterator_test() {
//        let db = MemoryManager::new();
//        let first_collection: Box<dyn DatabaseCollection> = db.create_collection("first");
//        build_state(&first_collection);
//        // ITER TEST
//        let mut iter = first_collection.rev_iter();
//        let (keys, data) = build_initial_data();
//        for i in (0..3).rev() {
//            let (key, val) = iter.next().unwrap();
//            assert_eq!(keys[i], key);
//            assert_eq!(data[i], val);
//        }
//        assert!(iter.next().is_none());
//    }
//
//    #[test]
//    fn iterator_with_various_collection_test() {
//        let db = MemoryManager::new();
//        let first_collection: Box<dyn DatabaseCollection<InnerDataType = Data>> =
//            db.create_collection("first");
//        let second_collection: Box<dyn DatabaseCollection<InnerDataType = Data>> =
//            db.create_collection("second");
//        build_state(&first_collection);
//        let result = second_collection.put(
//            "d",
//            Data {
//                id: 4,
//                value: "D".into(),
//            },
//        );
//        assert!(result.is_ok());
//        let result = second_collection.put(
//            "e",
//            Data {
//                id: 5,
//                value: "E".into(),
//            },
//        );
//        assert!(result.is_ok());
//        let mut iter = first_collection.iter(false);
//        let (keys, data) = build_initial_data();
//        for i in 0..3 {
//            let (key, val) = iter.next().unwrap();
//            assert_eq!(keys[i], key);
//            assert_eq!(data[i], val);
//        }
//        assert!(iter.next().is_none());
//
//        let mut iter = second_collection.iter(false);
//        let (keys, data) = (
//            vec!["d", "e"],
//            vec![
//                Data {
//                    id: 4,
//                    value: "D".into(),
//                },
//                Data {
//                    id: 5,
//                    value: "E".into(),
//                },
//            ],
//        );
//        for i in 0..2 {
//            let (key, val) = iter.next().unwrap();
//            assert_eq!(keys[i], key);
//            assert_eq!(data[i], val);
//        }
//        assert!(iter.next().is_none());
//    }
//
//    #[test]
//    fn rev_iterator_with_various_collection_test() {
//        let db = MemoryManager::new();
//        let first_collection: Box<dyn DatabaseCollection<InnerDataType = Data>> =
//            db.create_collection("first");
//        let second_collection: Box<dyn DatabaseCollection<InnerDataType = Data>> =
//            db.create_collection("second");
//        build_state(&first_collection);
//        let result = second_collection.put(
//            "d",
//            Data {
//                id: 4,
//                value: "D".into(),
//            },
//        );
//        assert!(result.is_ok());
//        let result = second_collection.put(
//            "e",
//            Data {
//                id: 5,
//                value: "E".into(),
//            },
//        );
//        assert!(result.is_ok());
//        let mut iter = first_collection.rev_iter();
//        let (keys, data) = build_initial_data();
//        for i in (0..3).rev() {
//            let (key, val) = iter.next().unwrap();
//            assert_eq!(keys[i], key);
//            assert_eq!(data[i], val);
//        }
//        assert!(iter.next().is_none());
//
//        let mut iter = second_collection.rev_iter();
//        let (keys, data) = (
//            vec!["d", "e"],
//            vec![
//                Data {
//                    id: 4,
//                    value: "D".into(),
//                },
//                Data {
//                    id: 5,
//                    value: "E".into(),
//                },
//            ],
//        );
//        for i in (0..2).rev() {
//            let (key, val) = iter.next().unwrap();
//            assert_eq!(keys[i], key);
//            assert_eq!(data[i], val);
//        }
//        assert!(iter.next().is_none());
//    }
//
//    #[test]
//    fn iteration_with_partitions_test() {
//        let db = MemoryManager::new();
//        let first_collection: Box<dyn DatabaseCollection<InnerDataType = Data>> =
//            db.create_collection("first");
//        let first_inner = first_collection.partition("inner1");
//        let second_inner = first_inner.partition("inner2");
//        first_collection
//            .put(
//                "a",
//                Data {
//                    id: 0,
//                    value: "A".into(),
//                },
//            )
//            .unwrap();
//        first_inner
//            .put(
//                "b",
//                Data {
//                    id: 0,
//                    value: "B".into(),
//                },
//            )
//            .unwrap();
//        second_inner
//            .put(
//                "c",
//                Data {
//                    id: 0,
//                    value: "C".into(),
//                },
//            )
//            .unwrap();
//        let mut iter = second_inner.iter();
//        let item = iter.next().unwrap();
//        assert_eq!(&item.0, "c");
//        assert!(iter.next().is_none());
//
//        let mut iter = first_inner.iter();
//        let item = iter.next().unwrap();
//        assert_eq!(&item.0, "b");
//        let item = iter.next().unwrap();
//        assert_eq!(&item.0, "inner2\u{10ffff}c");
//        assert!(iter.next().is_none());
//
//        let mut iter = first_collection.iter(false);
//        let item = iter.next().unwrap();
//        assert_eq!(&item.0, "a");
//        let item = iter.next().unwrap();
//        assert_eq!(&item.0, "inner1\u{10ffff}b");
//        let item = iter.next().unwrap();
//        assert_eq!(&item.0, "inner1\u{10ffff}inner2\u{10ffff}c");
//        assert!(iter.next().is_none());
//    }
//
//    #[test]
//    fn rev_iteration_with_partitions_test() {
//        let db = MemoryManager::new();
//        let first_collection: Box<dyn DatabaseCollection<InnerDataType = Data>> =
//            db.create_collection("first");
//        let first_inner = first_collection.partition("inner1");
//        let second_inner = first_inner.partition("inner2");
//        first_collection
//            .put(
//                "a",
//                Data {
//                    id: 0,
//                    value: "A".into(),
//                },
//            )
//            .unwrap();
//        first_inner
//            .put(
//                "b",
//                Data {
//                    id: 0,
//                    value: "B".into(),
//                },
//            )
//            .unwrap();
//        second_inner
//            .put(
//                "c",
//                Data {
//                    id: 0,
//                    value: "C".into(),
//                },
//            )
//            .unwrap();
//        let mut iter = second_inner.rev_iter();
//        let item = iter.next().unwrap();
//        assert_eq!(&item.0, "c");
//        assert!(iter.next().is_none());
//
//        let mut iter = first_inner.rev_iter();
//        let item = iter.next().unwrap();
//        assert_eq!(&item.0, "inner2\u{10ffff}c");
//        let item = iter.next().unwrap();
//        assert_eq!(&item.0, "b");
//        assert!(iter.next().is_none());
//
//        let mut iter = first_collection.rev_iter();
//        let item = iter.next().unwrap();
//        assert_eq!(&item.0, "inner1\u{10ffff}inner2\u{10ffff}c");
//        let item = iter.next().unwrap();
//        assert_eq!(&item.0, "inner1\u{10ffff}b");
//        let item = iter.next().unwrap();
//        assert_eq!(&item.0, "a");
//        assert!(iter.next().is_none());
//    }
//}