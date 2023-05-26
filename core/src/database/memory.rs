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
    fn iter(&self) -> MemoryIterator {
        MemoryIterator::new(&self)
    }

    fn rev_iter(&self) -> RevMemoryIterator {
        RevMemoryIterator::new(&self)
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
        let lock = self.data.write().unwrap();
        let db: Arc<DataStore> = match lock.get(identifier) {
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
    fn get(&self, key: &str) -> Result<Vec<u8>, Error> {
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
        _prefix: String
    ) -> Box<dyn Iterator<Item = (String, Vec<u8>)> + 'a> {
        if reverse {
            Box::new(self.data.rev_iter())
        } else {
            Box::new(self.data.iter())
        }
    }
}

type GuardIter<'a, K, V> = (Arc<RwLockReadGuard<'a, BTreeMap<K, V>>>, Iter<'a, K, V>);

pub struct MemoryIterator<'a> {
    map: &'a DataStore,
    current: Option<GuardIter<'a, String, Vec<u8>>>
}

impl<'a> MemoryIterator<'a> {
    fn new(map: &'a DataStore) -> Self {
        Self {
            map,
            current: None
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
            return Some((key.clone(), val.clone()));
        }
    }
}

type GuardRevIter<'a> = (
    Arc<RwLockReadGuard<'a, BTreeMap<String, Vec<u8>>>>,
    Rev<Iter<'a, String, Vec<u8>>>,
);

pub struct RevMemoryIterator<'a> {
    map: &'a DataStore,
    current: Option<GuardRevIter<'a>>
}

impl<'a> RevMemoryIterator<'a> {
    fn new(map: &'a DataStore) -> Self {
        Self {
            map,
            current: None
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
            return Some((key.clone(), val.clone()));
        }
    }
}

unsafe fn change_lifetime_const<'a, 'b, T>(x: &'a T) -> &'b T {
    &*(x as *const T)
}

#[cfg(test)]
mod test {
    use super::{MemoryManager, MemoryCollection};
    use crate::{database::DatabaseCollection, DatabaseManager};
    use serde::{Deserialize, Serialize};
    use super::{Error};

    #[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
    struct Data {
        id: usize,
        value: String,
    }

    fn get_data() -> Result<Vec<Vec<u8>>, Error> {
        let data1 = Data {
            id: 1,
            value: "A".into(),
        };
        let data2 = Data {
            id: 2,
            value: "B".into(),
        };
        let data3 = Data {
            id: 3,
            value: "C".into(),
        };
        let Ok(data1) = bincode::serialize::<Data>(&data1) else {
            return Err(Error::SerializeError);
        };
        let Ok(data2) = bincode::serialize::<Data>(&data2) else {
            return Err(Error::SerializeError);
        };
        let Ok(data3) = bincode::serialize::<Data>(&data3) else {
            return Err(Error::SerializeError);
        };
        Ok(vec![data1, data2, data3])
    }

    #[test]
    fn basic_operations_test() {
        let db = MemoryManager::new();
        let first_collection = db.create_collection("first");
        let data = get_data().unwrap();
        // PUT & GET Operations
        // PUT
        let result = first_collection.put(
            "a",
            data[0].clone(),
        );
        assert!(result.is_ok());
        let result = first_collection.put(
            "b",
            data[1].clone(),
        );
        assert!(result.is_ok());
        let result = first_collection.put(
            "c",
            data[2].clone(),
        );
        assert!(result.is_ok());
        // GET
        let result = first_collection.get("a");
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            data[0]
        );
        let result = first_collection.get("b");
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            data[1]
        );
        let result = first_collection.get("c");
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            data[2]
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
        let first_collection = db.create_collection("first");
        let second_collection = db.create_collection("second");
        let data = get_data().unwrap();
        // PUT UNIQUE ENTRIES IN EACH PARTITION
        let result = first_collection.put(
            "a",
            data[0].to_owned(),
        );
        assert!(result.is_ok());
        let result = second_collection.put(
            "b",
            data[1].to_owned(),
        );
        assert!(result.is_ok());
        // TRYING TO GET ENTRIES FROM A DIFFERENT PARTITION
        let result = first_collection.get("b");
        assert!(result.is_err());
        let result = second_collection.get("a");
        assert!(result.is_err());
    }

    fn build_state(collection: &MemoryCollection) {
        let data = get_data().unwrap();
        let result = collection.put(
            "a",
            data[0].to_owned()
        );
        assert!(result.is_ok());
        let result = collection.put(
            "b",
            data[1].to_owned()
        );
        assert!(result.is_ok());
        let result = collection.put(
            "c",
            data[2].to_owned()
        );
        assert!(result.is_ok());
    }

    fn build_initial_data() -> (Vec<&'static str>, Vec<Vec<u8>>) {
        let keys = vec!["a", "b", "c"];
        let data = get_data().unwrap();
        let values = vec![data[0].to_owned(), data[1].to_owned(), data[2].to_owned()];
        (keys, values)
    }

    #[test]
    fn iterator_test() {
        let db = MemoryManager::new();
        let first_collection = db.create_collection("first");
        build_state(&first_collection);
        // ITER TEST
        let mut iter = first_collection.iter(false, "first".to_string());
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
        let first_collection = db.create_collection("first");
        build_state(&first_collection);
        // ITER TEST
        let mut iter = first_collection.iter(true, "first".to_string());
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
        let first_collection = db.create_collection("first");
        let second_collection = db.create_collection("second");
        build_state(&first_collection);
        let data4 = Data {
            id: 4,
            value: "D".into(),
        };
        let data5 = Data {
            id: 5,
            value: "E".into(),
        };
        let Ok(data4) = bincode::serialize::<Data>(&data4) else {
            panic!();
        };
        let Ok(data5) = bincode::serialize::<Data>(&data5) else {
            panic!();
        };
        let result = second_collection.put(
            "d",
            data4.clone()
        );
        assert!(result.is_ok());
        let result = second_collection.put(
            "e",
            data5.clone()
        );
        assert!(result.is_ok());

        let mut iter = second_collection.iter(false, "second".to_string());
        let (keys, data) = (
            vec!["d", "e"],
            vec![data4, data5]
        );
        for i in 0..2 {
            let (key, val) = iter.next().unwrap();
            assert_eq!(keys[i], key);
            assert_eq!(data[i], val);
        }
        assert!(iter.next().is_none());

        let mut iter = first_collection.iter(false, "first".to_string());
        let (keys, data) = build_initial_data();
        for i in 0..3 {
            let (key, val) = iter.next().unwrap();
            assert_eq!(keys[i], key);
            assert_eq!(data[i], val);
        }
        assert!(iter.next().is_none()); 
    }

    #[test]
    fn rev_iterator_with_various_collection_test() {
        let db = MemoryManager::new();
        let first_collection = db.create_collection("first");
        let second_collection = db.create_collection("second");
        build_state(&first_collection);
        let data4 = Data {
            id: 4,
            value: "D".into(),
        };
        let data5 = Data {
            id: 5,
            value: "E".into(),
        };
        let Ok(data4) = bincode::serialize::<Data>(&data4) else {
            panic!();
        };
        let Ok(data5) = bincode::serialize::<Data>(&data5) else {
            panic!();
        };
        let result = second_collection.put(
            "d",
            data4.clone()
        );
        assert!(result.is_ok());
        let result = second_collection.put(
            "e",
            data5.clone()
        );
        assert!(result.is_ok());

        let mut iter = second_collection.iter(true, "second".to_string());
        let (keys, data) = (
            vec!["d", "e"],
            vec![data4, data5],
        );
        for i in (0..2).rev() {
            let (key, val) = iter.next().unwrap();
            assert_eq!(keys[i], key);
            assert_eq!(data[i], val);
        }
        assert!(iter.next().is_none());

        let mut iter = first_collection.iter(true, "first".to_string());
        let (keys, data) = build_initial_data();
        for i in (0..3).rev() {
            let (key, val) = iter.next().unwrap();
            assert_eq!(keys[i], key);
            assert_eq!(data[i], val);
        }
        assert!(iter.next().is_none());
    }

}