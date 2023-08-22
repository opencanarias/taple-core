use db_key;
use leveldb::options::Options as LevelDBOptions;
use leveldb::{
    database::Database,
    iterator::{Iterable, Iterator as LevelIterator, LevelDBIterator, RevIterator},
    kv::KV,
};
use std::cell::Cell;
use std::path::Path;
use std::sync::Arc;
use taple_core::{
    test_database_manager_trait, DatabaseCollection, DatabaseManager, DbError as Error,
};

#[derive(Debug, PartialEq, Eq)]
pub struct StringKey(pub String);
impl db_key::Key for StringKey {
    fn from_u8(key: &[u8]) -> Self {
        Self(String::from_utf8(key.to_vec()).unwrap())
    }

    fn as_slice<T, F: Fn(&[u8]) -> T>(&self, f: F) -> T {
        let dst = self.0.as_bytes();
        f(&dst)
    }
}

#[derive(Clone, Copy)]
struct ReadOptions {
    fill_cache: bool,
    verify_checksums: bool,
}

impl<'a, K> From<ReadOptions> for leveldb::options::ReadOptions<'a, K>
where
    K: db_key::Key,
{
    fn from(item: ReadOptions) -> Self {
        let mut options = leveldb::options::ReadOptions::new();
        options.fill_cache = item.fill_cache;
        options.verify_checksums = item.verify_checksums;
        options
    }
}

fn get_initial_options() -> LevelDBOptions {
    let mut db_options = LevelDBOptions::new();
    db_options.create_if_missing = true;
    db_options
}

pub fn open_db(path: &Path) -> Arc<Database<StringKey>> {
    let db_options = get_initial_options();
    if let Ok(db) = Database::<StringKey>::open(path, db_options) {
        Arc::new(db)
    } else {
        panic!("Error opening DB with comparator")
    }
}

pub struct SyncCell<T>(Cell<T>);
unsafe impl<T> Sync for SyncCell<T> {}

pub struct LevelDBManager {
    db: Arc<Database<StringKey>>,
}

#[allow(dead_code)]
impl LevelDBManager {
    pub fn new(db: Arc<Database<StringKey>>) -> Self {
        Self { db }
    }
}

impl DatabaseManager<LDBCollection> for LevelDBManager {
    fn default() -> Self {
        let temp_dir = tempfile::tempdir().unwrap();
        let db = open_db(temp_dir.path());
        Self { db }
    }

    fn create_collection(&self, _identifier: &str) -> LDBCollection {
        LDBCollection {
            data: self.db.clone(),
            read_options: SyncCell(Cell::new(None)),
            write_options: SyncCell(Cell::new(None)),
        }
    }
}

pub struct LDBCollection {
    data: Arc<Database<StringKey>>,
    read_options: SyncCell<Option<ReadOptions>>,
    write_options: SyncCell<Option<leveldb::options::WriteOptions>>,
}

impl LDBCollection {
    fn generate_key(&self, key: &str) -> StringKey {
        StringKey(key.to_string())
    }

    pub fn get_read_options(&self) -> leveldb::options::ReadOptions<StringKey> {
        if let Some(options) = self.read_options.0.get() {
            return leveldb::options::ReadOptions::from(options);
        } else {
            return leveldb::options::ReadOptions::new();
        }
    }

    fn get_write_options(&self) -> leveldb::options::WriteOptions {
        if let Some(options) = self.write_options.0.get() {
            return options;
        } else {
            let mut write_options = leveldb::options::WriteOptions::new();
            write_options.sync = true;
            return write_options;
        }
    }
}

impl DatabaseCollection for LDBCollection {
    fn get(&self, key: &str) -> Result<Vec<u8>, Error> {
        let key = self.generate_key(key);
        let result = self.data.get(self.get_read_options(), key);
        match result {
            Err(_) => Err(Error::EntryNotFound),
            Ok(data) => match data {
                Some(value) => Ok(value),
                None => Err(Error::EntryNotFound),
            },
        }
    }

    fn put(&self, key: &str, data: Vec<u8>) -> Result<(), Error> {
        let key = self.generate_key(key);
        let _result = self.data.put(self.get_write_options(), key, &data);
        Ok(())
    }

    fn del(&self, key: &str) -> Result<(), Error> {
        let key = self.generate_key(key);
        let _result = self.data.delete(self.get_write_options(), key);
        Ok(())
    }

    fn iter<'a>(
        &'a self,
        reverse: bool,
        prefix: String,
    ) -> Box<dyn Iterator<Item = (String, Vec<u8>)> + 'a> {
        if reverse {
            let iter = self.data.iter(self.get_read_options()).reverse();
            iter.seek(&StringKey(format!(
                "{}{}{}",
                prefix.clone(),
                char::MAX,
                char::MAX
            )));
            let mut alt_iter = iter.peekable();
            let iter = if let Some(_) = alt_iter.peek() {
                let mut iter = self.data.iter(self.get_read_options()).reverse();
                iter.seek(&StringKey(format!(
                    "{}{}{}",
                    prefix.clone(),
                    char::MAX,
                    char::MAX
                )));
                iter.advance();
                iter
            } else {
                self.data.iter(self.get_read_options()).reverse()
            };
            Box::new(RevLDBIterator::new(iter, prefix))
        } else {
            Box::new(LDBIterator::new(
                self.data.iter(self.get_read_options()),
                prefix,
            ))
        }
    }
}

pub struct LDBIterator<'a> {
    iter: LevelIterator<'a, StringKey>,
    table_name: String,
}

impl<'a> LDBIterator<'a> {
    pub fn new(iter: LevelIterator<'a, StringKey>, table_name: String) -> Self {
        iter.seek(&StringKey(table_name.clone()));
        Self { iter, table_name }
    }
}

impl<'a> Iterator for LDBIterator<'a> {
    type Item = (String, Vec<u8>);
    fn next(&mut self) -> Option<(String, Vec<u8>)> {
        let item = self.iter.next();
        let Some(item) = item else {
                return None;
            };
        let key = {
            let StringKey(value) = item.0;
            if !value.starts_with(&self.table_name) {
                return None;
            }
            value.replace(&self.table_name, "")
        };
        Some((key, item.1))
    }
}

pub struct RevLDBIterator<'a> {
    iter: RevIterator<'a, StringKey>,
    table_name: String,
}

impl<'a> RevLDBIterator<'a> {
    pub fn new(iter: RevIterator<'a, StringKey>, table_name: String) -> Self {
        Self { iter, table_name }
    }
}

impl<'a> Iterator for RevLDBIterator<'a> {
    type Item = (String, Vec<u8>);
    fn next(&mut self) -> Option<Self::Item> {
        let item = self.iter.next();
        let Some(item) = item else {
                return None;
            };
        let key = {
            let StringKey(value) = item.0;
            if !value.starts_with(&self.table_name) {
                return None;
            }
            value.replace(&self.table_name, "")
        };
        Some((key, item.1))
    }
}

test_database_manager_trait! {
    unit_test_leveldb_manager:LevelDBManager:LDBCollection
}
