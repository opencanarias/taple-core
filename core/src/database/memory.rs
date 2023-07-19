use super::Error;
use crate::{test_database_manager_trait, DatabaseCollection, DatabaseManager};
use std::{
    collections::{btree_map::Iter, BTreeMap, HashMap},
    iter::Rev,
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
};

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
    fn iter(&self, prefix: String) -> MemoryIterator {
        MemoryIterator::new(&self, prefix)
    }

    fn rev_iter(&self, prefix: String) -> RevMemoryIterator {
        RevMemoryIterator::new(&self, prefix)
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
    fn default() -> Self {
        Self {
            data: RwLock::new(HashMap::new()),
        }
    }

    fn create_collection(&self, _identifier: &str) -> MemoryCollection {
        let mut lock = self.data.write().unwrap();
        let db: Arc<DataStore> = match lock.get("") {
            Some(map) => map.clone(),
            None => {
                let db: Arc<DataStore> = Arc::new(DataStore::new());
                lock.insert("".to_string(), db.clone());
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
        prefix: String,
    ) -> Box<dyn Iterator<Item = (String, Vec<u8>)> + 'a> {
        if reverse {
            Box::new(self.data.rev_iter(prefix))
        } else {
            Box::new(self.data.iter(prefix))
        }
    }
}

type GuardIter<'a, K, V> = (Arc<RwLockReadGuard<'a, BTreeMap<K, V>>>, Iter<'a, K, V>);

pub struct MemoryIterator<'a> {
    map: &'a DataStore,
    current: Option<GuardIter<'a, String, Vec<u8>>>,
    table_name: String,
}

impl<'a> MemoryIterator<'a> {
    fn new(map: &'a DataStore, table_name: String) -> Self {
        Self {
            map,
            current: None,
            table_name,
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
            let Some(item) = iter.next() else {
                return None;
            };
            let key = {
                let value = item.0.clone();
                if !value.starts_with(&self.table_name) {
                    return None;
                }
                value.replace(&self.table_name, "")
            };
            return Some((key, item.1.clone()));
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
    table_name: String,
}

impl<'a> RevMemoryIterator<'a> {
    fn new(map: &'a DataStore, table_name: String) -> Self {
        Self {
            map,
            current: None,
            table_name,
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
            let Some(item) = iter.next() else {
                return None;
            };
            let key = {
                let value = item.0.clone();
                if !value.starts_with(&self.table_name) {
                    return None;
                }
                value.replace(&self.table_name, "")
            };
            return Some((key, item.1.clone()));
        }
    }
}

unsafe fn change_lifetime_const<'a, 'b, T>(x: &'a T) -> &'b T {
    &*(x as *const T)
}

test_database_manager_trait! {
    unit_test_memory_manager:crate::MemoryManager:MemoryCollection
}
