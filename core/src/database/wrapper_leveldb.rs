use std::cell::{Cell, RefCell};
use std::cmp::Ordering;
use std::rc::Rc;

use leveldb::comparator::OrdComparator;
use leveldb::database::Database as LevelDataBase;
use std::sync::Arc as core_Arc;

type LevelDBShared<K> = core_Arc<LevelDataBase<K>>;

pub fn open_db<K: db_key::Key>(
    path: &std::path::Path,
    db_options: options::Options,
) -> Result<LevelDataBase<K>, leveldb::database::error::Error> {
    Ok(leveldb::database::Database::<K>::open(path, db_options)?)
}

pub fn open_db_with_comparator<K: db_key::Key + Ord>(
    path: &std::path::Path,
    db_options: options::Options,
    comparator: OrdComparator<K>,
) -> Result<LevelDataBase<K>, leveldb::database::error::Error> {
    Ok(leveldb::database::Database::<K>::open_with_comparator(
        path, db_options, comparator,
    )?)
}

use db_key;
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

fn last_index_of(sttr: &str, c: char) -> Option<usize> {
    sttr.rfind(c)
}

fn ord_by_len(a: &Vec<&str>, b: &Vec<&str>) -> std::cmp::Ordering {
    if a.len() > b.len() {
        std::cmp::Ordering::Greater
    } else if a.len() < b.len() {
        std::cmp::Ordering::Less
    } else {
        std::cmp::Ordering::Equal
    }
}

impl PartialOrd for StringKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        // let last_separator_index_self = last_index_of(&self.0, char::MAX).expect("Has Separator");
        // let last_separator_index_other = last_index_of(&other.0, char::MAX).expect("Has Separator");
        // let self_prefix = &self.0[..last_separator_index_self];
        // let other_prefix = &other.0[..last_separator_index_other];
        // match self_prefix.partial_cmp(other_prefix) {
        //     Some(result) => match result {
        //         Ordering::Less => Some(Ordering::Less),
        //         Ordering::Greater => Some(Ordering::Greater),
        //         Ordering::Equal => self.0[last_separator_index_self..]
        //         .partial_cmp(&other.0[last_separator_index_other..]),
        //     },
        //     None => None,
        // }
        let splited_self: Vec<&str> = self.0.split(char::MAX).collect();
        let splited_other: Vec<&str> = other.0.split(char::MAX).collect();
        let len = splited_self.len();
        let odr_by_len = ord_by_len(&splited_self, &splited_other);
        if odr_by_len != std::cmp::Ordering::Equal {
            Some(odr_by_len)
        } else {
            for i in 0..len {
                let pcmp = check_partial_cmp(splited_self[i], splited_other[i]);
                if pcmp != Some(std::cmp::Ordering::Equal) {
                    return pcmp;
                }
            }
            Some(Ordering::Equal)
        }
    }
}

fn check_partial_cmp(a: &str, b: &str) -> Option<std::cmp::Ordering> {
    if a.len() > b.len() {
        Some(std::cmp::Ordering::Greater)
    } else if a.len() < b.len() {
        Some(std::cmp::Ordering::Less)
    } else {
        a.partial_cmp(b)
    }
}

impl Ord for StringKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // let last_separator_index_self = last_index_of(&self.0, char::MAX).expect("Has Separator");
        // let last_separator_index_other = last_index_of(&other.0, char::MAX).expect("Has Separator");
        // let self_prefix = &self.0[..last_separator_index_self];
        // let other_prefix = &other.0[..last_separator_index_other];
        // match self_prefix.cmp(other_prefix) {
        //     Ordering::Less => Ordering::Less,
        //     Ordering::Greater => Ordering::Greater,
        //     Ordering::Equal => {
        //         self.0[last_separator_index_self..].cmp(&other.0[last_separator_index_other..])
        //     }
        // }
        let splited_self: Vec<&str> = self.0.split(char::MAX).collect();
        let splited_other: Vec<&str> = other.0.split(char::MAX).collect();
        let len = splited_self.len();
        let odr_by_len = ord_by_len(&splited_self, &splited_other);
        if odr_by_len != std::cmp::Ordering::Equal {
            odr_by_len
        } else {
            for i in 0..len {
                let pcmp = check_cmp(splited_self[i], splited_other[i]);
                if pcmp != std::cmp::Ordering::Equal {
                    return pcmp;
                }
            }
            Ordering::Equal
        }
    }
}

fn check_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    if a.len() > b.len() {
        std::cmp::Ordering::Greater
    } else if a.len() < b.len() {
        std::cmp::Ordering::Less
    } else {
        a.cmp(&b)
    }
}

#[derive(Clone, Copy)]
struct ReadOptions {
    fill_cache: bool,
    verify_checksums: bool,
}

impl<'a, K> From<ReadOptions> for options::ReadOptions<'a, K>
where
    K: db_key::Key,
{
    fn from(item: ReadOptions) -> Self {
        let mut options = options::ReadOptions::new();
        options.fill_cache = item.fill_cache;
        options.verify_checksums = item.verify_checksums;
        options
    }
}

impl<'a, K> From<options::ReadOptions<'a, K>> for ReadOptions
where
    K: db_key::Key,
{
    fn from(item: options::ReadOptions<'a, K>) -> Self {
        ReadOptions {
            fill_cache: item.fill_cache,
            verify_checksums: item.verify_checksums,
        }
    }
}

pub struct SyncCell<T>(Cell<T>);
unsafe impl<T> Sync for SyncCell<T> {}

use serde::{de::DeserializeOwned, Serialize};
use std::marker::PhantomData;

pub struct WrapperLevelDB<K: db_key::Key, V: Serialize + DeserializeOwned> {
    pub(crate) db: LevelDBShared<K>,
    pub(crate) selected_table: String,
    read_options: SyncCell<Option<ReadOptions>>,
    write_options: SyncCell<Option<options::WriteOptions>>,
    separator: char,
    phantom: PhantomData<V>,
}

impl<K, V> WrapperLevelDB<K, V>
where
    K: db_key::Key,
    V: Serialize + DeserializeOwned,
{
    pub(crate) fn deserialize(bytes: Vec<u8>) -> Result<V, error::WrapperLevelDBErrors> {
        let result = bincode::deserialize::<V>(bytes.as_slice());
        if let Ok(value) = result {
            return Ok(value);
        } else {
            return Err(error::WrapperLevelDBErrors::DeserializeError);
        }
    }

    pub(crate) fn serialize(value: V) -> Result<Vec<u8>, error::WrapperLevelDBErrors> {
        if let Ok(bytes) = bincode::serialize(&value) {
            return Ok(bytes);
        } else {
            return Err(error::WrapperLevelDBErrors::SerializeError);
        };
    }
}

#[derive(PartialEq)]
pub enum CursorIndex {
    FromBeginning,
    FromEnding,
    FromKey(String),
}

use super::error;
use leveldb::iterator::{Iterable, LevelDBIterator};
use leveldb::{database::options, kv::KV};
impl<V> WrapperLevelDB<StringKey, V>
where
    V: Serialize + DeserializeOwned,
{
    pub fn new(db: LevelDBShared<StringKey>, table_name: &str) -> WrapperLevelDB<StringKey, V> {
        WrapperLevelDB {
            db: db.clone(),
            selected_table: String::from(table_name),
            read_options: SyncCell(Cell::new(None)),
            write_options: SyncCell(Cell::new(None)),
            separator: char::MAX,
            phantom: PhantomData::default(),
        }
    }

    pub fn partition(&self, subtable_name: &str) -> Self {
        // Create the concatenation
        let table_name = self.build_key(subtable_name);
        WrapperLevelDB {
            db: self.db.clone(),
            selected_table: table_name.0,
            read_options: SyncCell(self.read_options.0.clone()),
            write_options: SyncCell(self.write_options.0.clone()),
            separator: self.separator,
            phantom: PhantomData::default(),
        }
    }

    pub(crate) fn create_last_key(&self) -> String {
        let mut last_key = self.selected_table.clone();
        // Se hace dos veces porque con uno indicaría que es el primero (porque va nada a continuación, sin embargo 2 significa que es el último (primero de la siguiente))
        last_key.push(self.separator);
        last_key.push(self.separator);
        last_key
    }

    pub(crate) fn get_read_options(&self) -> options::ReadOptions<StringKey> {
        if let Some(options) = self.read_options.0.get() {
            return options::ReadOptions::from(options);
        } else {
            return options::ReadOptions::new();
        }
    }

    #[allow(dead_code)]
    pub fn set_read_options(&mut self, options: options::ReadOptions<StringKey>) {
        self.read_options
            .0
            .replace(Some(ReadOptions::from(options)));
    }

    fn get_write_options(&self) -> options::WriteOptions {
        if let Some(options) = self.write_options.0.get() {
            return options;
        } else {
            let mut write_options = options::WriteOptions::new();
            write_options.sync = true;
            return write_options;
        }
    }

    #[allow(dead_code)]
    pub fn set_write_options(&self, options: options::WriteOptions) {
        self.write_options.0.replace(Some(options));
    }

    fn build_key(&self, key: &str) -> StringKey {
        let table_name = self.selected_table.clone();
        let mut key_builder = String::with_capacity(table_name.len() + key.len() + 1);
        key_builder.push_str(&table_name);
        key_builder.push(self.separator);
        key_builder.push_str(&key);

        StringKey(key_builder)
    }

    pub fn get_table_name(&self) -> String {
        let table_name = self.selected_table.clone();
        let mut key_builder = String::with_capacity(table_name.len() + 1);
        key_builder.push_str(&table_name);
        let last_char: String = self.separator.into();
        key_builder.push_str(&last_char);
        key_builder
    }

    pub fn put(&self, key: &str, value: V) -> Result<(), error::WrapperLevelDBErrors> {
        let key = self.build_key(key);
        let value = WrapperLevelDB::<StringKey, V>::serialize(value)?;

        Ok({
            self.db
                .put(self.get_write_options(), key, value.as_slice())?
        })
    }

    #[allow(dead_code)]
    pub fn get_bytes(
        &self,
        key: &str,
    ) -> Result<leveldb::database::bytes::Bytes, error::WrapperLevelDBErrors> {
        let key = self.build_key(key);
        let result = { self.db.get_bytes(self.get_read_options(), key)? };
        if let Some(bytes) = result {
            return Ok(bytes);
        } else {
            return Err(error::WrapperLevelDBErrors::EntryNotFoundError);
        }
    }

    pub fn get(&self, key: &str) -> Result<V, error::WrapperLevelDBErrors> {
        let key = self.build_key(key);
        let result = { self.db.get(self.get_read_options(), key)? };
        if let Some(bytes) = result {
            return Ok(WrapperLevelDB::<StringKey, V>::deserialize(bytes)?);
        } else {
            return Err(error::WrapperLevelDBErrors::EntryNotFoundError);
        }
    }

    #[allow(dead_code)]
    pub fn update(&self, key: &str, value: V) -> Result<V, error::WrapperLevelDBErrors> {
        // Check that something exists
        let old_value = self.get(key)?;
        // If it exists, we modify it
        let key = self.build_key(key);
        let value = if let Ok(bytes) = bincode::serialize(&value) {
            bytes
        } else {
            return Err(error::WrapperLevelDBErrors::SerializeError);
        };
        // Update
        self.db
            .put(self.get_write_options(), key, value.as_slice())?;
        Ok(old_value)
    }

    pub fn del(&self, key: &str) -> Result<Option<V>, error::WrapperLevelDBErrors> {
        let old_value = if let Ok(value) = self.get(key) {
            Some(value)
        } else {
            None
        };
        let key = self.build_key(key);
        let write_opts = self.get_write_options();
        self.db.delete(write_opts, key)?;
        Ok(old_value)
    }

    pub fn get_all(&self) -> Vec<(StringKey, V)> {
        let iter = self.db.iter(self.get_read_options());
        let table_name = self.get_table_name();

        iter.seek(&StringKey(self.selected_table.clone()));
        iter.map_while(|(key, bytes)| {
            // Stop when it returns None
            if key.0.starts_with(&table_name) {
                let key = {
                    let StringKey(value) = key;
                    // Remove the table name from the key
                    StringKey(value.replace(&table_name, ""))
                };
                // Perform deserialization to obtain the stored structure from bytes
                let value = WrapperLevelDB::<StringKey, V>::deserialize(bytes).unwrap();
                Some((key, value))
            } else {
                None
            }
        })
        .collect()
    }

    fn get_range_aux<I: Iterator<Item = (StringKey, Vec<u8>)>, F: Fn(&V) -> bool>(
        &self,
        iter: I,
        quantity: isize,
        filter: Option<F>,
    ) -> Vec<(StringKey, V)> {
        let count = Rc::new(RefCell::new(0usize));
        let table_name = self.get_table_name();
        let closure = |value: (StringKey, Vec<u8>)| {
            // Stop when it returns None
            let (key, bytes) = value;
            if key.0.starts_with(&table_name)
                && (quantity == 0 || *count.borrow() < quantity as usize)
            {
                let key = {
                    let StringKey(value) = key;
                    // Remove the table name from the key
                    StringKey(value.replace(&table_name, ""))
                };
                // Perform deserialization to obtain the stored structure from bytes
                let value = WrapperLevelDB::<StringKey, V>::deserialize(bytes).unwrap();
                count.replace_with(|&mut old| old + 1);
                return Some((key, value));
            } else {
                None
            }
        };
        if filter.is_some() {
            let filter = filter.unwrap();
            iter.map_while(closure)
                .filter(|(_, data)| {
                    let result = filter(data);
                    if !result {
                        count.replace_with(|&mut old| if old > 0 { old - 1 } else { old });
                    }
                    result
                })
                .collect()
        } else {
            iter.map_while(closure).collect()
        }
    }

    pub fn get_range<F: Fn(&V) -> bool>(
        &self,
        cursor: &CursorIndex,
        quantity: isize,
        filter: Option<F>,
    ) -> Vec<(StringKey, V)> {
        let iter = self.db.iter(self.get_read_options());
        let table_name = self.get_table_name();

        match cursor {
            CursorIndex::FromBeginning => {
                if quantity < 0 {
                    return vec![];
                }
                iter.seek(&StringKey(table_name.clone()));
                return self.get_range_aux(iter, quantity, filter);
            }
            CursorIndex::FromEnding => {
                if quantity < 0 {
                    return vec![];
                }
                let mut iter = iter.reverse();
                iter.advance();
                iter.seek(&StringKey(self.create_last_key()));
                return self.get_range_aux(iter, quantity, filter);
            }
            CursorIndex::FromKey(key) => {
                let key = self.build_key(&key);
                if quantity < 0 {
                    let iter = iter.reverse();
                    iter.seek(&key);
                    return self.get_range_aux(iter, quantity.abs(), filter);
                } else {
                    iter.seek(&key);
                    return self.get_range_aux(iter, quantity, filter);
                }
            }
        };
    }

    #[allow(dead_code)]
    pub fn get_count(&self) -> usize {
        let mut iter = self.db.keys_iter(self.get_read_options());
        let first_key = StringKey(self.get_table_name());
        let mut count = 0;
        iter.seek(&first_key);
        // Take the index of the first key of our 'table'....
        iter.any(|key| {
            if key.0.starts_with(&first_key.0) {
                count += 1;
                false
            } else {
                true
            }
        });
        count
    }
}
