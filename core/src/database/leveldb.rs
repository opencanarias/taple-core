use std::marker::PhantomData;

use leveldb::iterator::{Iterable, Iterator as LevelIterator, LevelDBIterator, RevIterator};
use serde::{de::DeserializeOwned, Serialize};

use self::wrapper_leveldb::StringKey;
use super::{
    error::{Error, WrapperLevelDBErrors},
    wrapper_leveldb, DatabaseCollection, DatabaseManager,
};

struct LevelDB {
    db: std::sync::Arc<leveldb::database::Database<StringKey>>,
}

impl DatabaseManager for LevelDB {
    fn create_collection<V>(
        &self,
        identifier: &str,
    ) -> Box<dyn DatabaseCollection<InnerDataType = V>>
    where
        V: Serialize + DeserializeOwned + 'static,
    {
        Box::new(wrapper_leveldb::WrapperLevelDB::<StringKey, V>::new(
            self.db.clone(),
            identifier,
        ))
    }
}

impl LevelDB {
    pub fn new(db: std::sync::Arc<leveldb::database::Database<StringKey>>) -> Self {
        Self { db }
    }
}

impl<V: Serialize + DeserializeOwned> DatabaseCollection
    for wrapper_leveldb::WrapperLevelDB<StringKey, V>
{
    type InnerDataType = V;

    fn put(&self, key: &str, data: Self::InnerDataType) -> Result<(), Error> {
        let result = self.put(key, data);
        match result {
            Ok(_) => Ok(()),
            Err(WrapperLevelDBErrors::SerializeError) => Err(Error::SerializeError),
            Err(WrapperLevelDBErrors::LevelDBError { source }) => {
                Err(Error::CustomError(Box::new(source)))
            }
            Err(_) => unreachable!(),
        }
    }

    fn get(&self, key: &str) -> Result<Self::InnerDataType, Error> {
        let result = self.get(key);
        match result {
            Err(WrapperLevelDBErrors::DeserializeError) => Err(Error::DeserializeError),
            Err(WrapperLevelDBErrors::EntryNotFoundError) => Err(Error::EntryNotFound),
            Err(WrapperLevelDBErrors::LevelDBError { source }) => {
                Err(Error::CustomError(Box::new(source)))
            }
            Ok(data) => Ok(data),
            _ => unreachable!(),
        }
    }

    fn del(&self, key: &str) -> Result<(), Error> {
        let result = self.del(key);
        if let Err(WrapperLevelDBErrors::LevelDBError { source }) = result {
            return Err(Error::CustomError(Box::new(source)));
        }
        Ok(())
    }

    fn iter<'a>(&'a self) -> Box<dyn Iterator<Item = (String, Self::InnerDataType)> + 'a> {
        let iter = self.db.iter(self.get_read_options());
        let table_name = self.get_table_name();
        Box::new(DbIterator::new(
            iter,
            table_name,
            self.selected_table.clone(),
        ))
    }

    fn rev_iter<'a>(&'a self) -> Box<dyn Iterator<Item = (String, Self::InnerDataType)> + 'a> {
        let mut iter = self.db.iter(self.get_read_options());
        let mut iter = iter.reverse();
        iter.advance();
        iter.seek(&StringKey(self.create_last_key()));
        let table_name = self.get_table_name();
        Box::new(RevDbIterator::new(
            iter,
            table_name,
            self.selected_table.clone(),
        ))
    }

    fn partition<'a>(
        &'a self,
        key: &str,
    ) -> Box<dyn DatabaseCollection<InnerDataType = Self::InnerDataType> + 'a> {
        Box::new(self.partition(&key))
    }
}

pub struct DbIterator<'a, V: Serialize + DeserializeOwned> {
    _tmp: PhantomData<V>,
    table_name: String,
    iter: LevelIterator<'a, StringKey>,
}

impl<'a, V: Serialize + DeserializeOwned> DbIterator<'a, V> {
    pub fn new(
        iter: LevelIterator<'a, StringKey>,
        table_name: String,
        selected_table: String,
    ) -> Self {
        iter.seek(&StringKey(table_name.clone()));
        Self {
            _tmp: PhantomData::default(),
            table_name,
            iter,
        }
    }
}

impl<'a, V: Serialize + DeserializeOwned> Iterator for DbIterator<'a, V> {
    type Item = (String, V);
    fn next(&mut self) -> Option<Self::Item> {
        let item = self.iter.next();
        let Some(item) = item else {
            return None;
        };
        if item.0 .0.starts_with(&self.table_name) {
            let value =
                wrapper_leveldb::WrapperLevelDB::<StringKey, V>::deserialize(item.1).unwrap();
            let key = {
                let StringKey(value) = item.0;
                // Remove the table name from the key
                value.replace(&self.table_name, "")
            };
            Some((key, value))
        } else {
            None
        }
    }
}

pub struct RevDbIterator<'a, V: Serialize + DeserializeOwned + 'a> {
    _tmp: PhantomData<V>,
    table_name: String,
    iter: RevIterator<'a, StringKey>,
}

impl<'a, V: Serialize + DeserializeOwned> RevDbIterator<'a, V> {
    pub fn new(
        iter: RevIterator<'a, StringKey>,
        table_name: String,
        selected_table: String,
    ) -> Self {
        // iter.seek(&StringKey(table_name.clone()));
        Self {
            _tmp: PhantomData::default(),
            table_name,
            iter,
        }
    }
}

impl<'a, V: Serialize + DeserializeOwned> Iterator for RevDbIterator<'a, V> {
    type Item = (String, V);
    fn next(&mut self) -> Option<Self::Item> {
        let item = self.iter.next();
        println!("ITEM {:?}", item);
        let Some(item) = item else {
            return None;
        };
        if item.0 .0.starts_with(&self.table_name) {
            let value =
                wrapper_leveldb::WrapperLevelDB::<StringKey, V>::deserialize(item.1).unwrap();
            let key = {
                let StringKey(value) = item.0;
                // Remove the table name from the key
                value.replace(&self.table_name, "")
            };
            Some((key, value))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod test {
    use std::{path::Path, vec};

    use crate::database::wrapper_leveldb::StringKey;
    use leveldb::comparator::OrdComparator;
    use leveldb::options::Options as LevelDBOptions;
    use serde::{Deserialize, Serialize};
    use tempfile::tempdir;

    use super::{DatabaseCollection, DatabaseManager, LevelDB};

    pub fn open_db(path: &Path) -> std::sync::Arc<leveldb::database::Database<StringKey>> {
        let mut db_options = LevelDBOptions::new();
        db_options.create_if_missing = true;
    
        if let Ok(db) = crate::commons::bd::level_db::wrapper_leveldb::open_db(path, db_options) {
            std::sync::Arc::new(db)
        } else {
            panic!("Error opening DB")
        }
    }

    pub fn open_db_with_comparator(
        path: &Path,
    ) -> std::sync::Arc<leveldb::database::Database<StringKey>> {
        let mut db_options = LevelDBOptions::new();
        db_options.create_if_missing = true;
        let comparator = OrdComparator::<StringKey>::new("taple_comparator".into());

        if let Ok(db) = crate::commons::bd::level_db::wrapper_leveldb::open_db_with_comparator(
            path, db_options, comparator,
        ) {
            std::sync::Arc::new(db)
        } else {
            panic!("Error opening DB with comparator")
        }
    }

    #[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
    struct Data {
        id: usize,
        value: String,
    }

    #[test]
    fn basic_operations_test() {
        let temp_dir = tempdir().unwrap();
        let db = LevelDB::new(open_db_with_comparator(temp_dir.path()));
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
        let temp_dir = tempdir().unwrap();
        let db = LevelDB::new(open_db_with_comparator(temp_dir.path()));
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
        let temp_dir = tempdir().unwrap();
        let db = LevelDB::new(open_db_with_comparator(temp_dir.path()));
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
        let temp_dir = tempdir().unwrap();
        let db = LevelDB::new(open_db_with_comparator(temp_dir.path()));
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
        let temp_dir = tempdir().unwrap();
        let db = LevelDB::new(open_db(temp_dir.path()));
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
        let temp_dir = tempdir().unwrap();
        let db = LevelDB::new(open_db_with_comparator(temp_dir.path()));
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
        let (keys, data) = (vec!["d", "e"], vec![Data {
            id: 4,
            value: "D".into(),
        }, Data {
            id: 5,
            value: "E".into(),
        }]);
        for i in 0..2 {
            let (key, val) = iter.next().unwrap();
            assert_eq!(keys[i], key);
            assert_eq!(data[i], val);
        }
        assert!(iter.next().is_none());
    }

    #[test]
    fn rev_iterator_with_various_collection_test() {
        let temp_dir = tempdir().unwrap();
        let db = LevelDB::new(open_db_with_comparator(temp_dir.path()));
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
        // println!("MIRA: {:?}", iter.next().unwrap());
        // println!("MIRA: {:?}", iter.next().unwrap());
        for i in (0..3).rev() {
            let (key, val) = iter.next().unwrap();
            assert_eq!(keys[i], key);
            assert_eq!(data[i], val);
        }
        assert!(iter.next().is_none());

        let mut iter = second_collection.rev_iter();
        let (keys, data) = (vec!["d", "e"], vec![Data {
            id: 4,
            value: "D".into(),
        }, Data {
            id: 5,
            value: "E".into(),
        }]);
        for i in (0..2).rev() {
            println!("A");
            let (key, val) = iter.next().unwrap();
            assert_eq!(keys[i], key);
            assert_eq!(data[i], val);
        }
        assert!(iter.next().is_none());
    }
}
