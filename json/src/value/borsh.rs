use borsh::{BorshDeserialize, BorshSerialize};
use std::{collections::BTreeMap, io::Read};

use crate::{Map, Value};

impl BorshSerialize for Value {
    #[inline]
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        match self {
            Value::Bool(data) => {
                BorshSerialize::serialize(&0u8, writer)?;
                BorshSerialize::serialize(&data, writer)
            }
            Value::Number(data) => {
                BorshSerialize::serialize(&1u8, writer)?;
                BorshSerialize::serialize(&data, writer)
            }
            Value::String(data) => {
                BorshSerialize::serialize(&2u8, writer)?;
                BorshSerialize::serialize(&data, writer)
            }
            Value::Array(data) => {
                BorshSerialize::serialize(&3u8, writer)?;
                BorshSerialize::serialize(&(data.len() as u32), writer)?;
                for element in data {
                    BorshSerialize::serialize(&element, writer)?;
                }
                Ok(())
            }
            Value::Object(data) => {
                BorshSerialize::serialize(&4u8, writer)?;
                BorshSerialize::serialize(&(data.len() as u32), writer)?;
                for (key, value) in data {
                    BorshSerialize::serialize(&key, writer)?;
                    BorshSerialize::serialize(&value, writer)?;
                }
                Ok(())
            }
            Value::Null => BorshSerialize::serialize(&5u8, writer),
        }
    }
}

impl BorshDeserialize for Value {
    #[inline]
    fn deserialize_reader<R: Read>(reader: &mut R) -> std::io::Result<Self> {
        let order: u8 = BorshDeserialize::deserialize_reader(reader)?;
        match order {
            0 => {
                let data: bool = BorshDeserialize::deserialize_reader(reader)?;
                Ok(Value::Bool(data))
            }
            1 => {
                let data = BorshDeserialize::deserialize_reader(reader)?;
                Ok(Value::Number(data))
            }
            2 => {
                let data: String = BorshDeserialize::deserialize_reader(reader)?;
                Ok(Value::String(data))
            }
            3 => {
                let len = u32::deserialize_reader(reader)?;
                if len == 0 {
                    Ok(Value::Array(Vec::new()))
                } else {
                    let mut result = Vec::with_capacity(len as usize);
                    for _ in 0..len {
                        result.push(Value::deserialize_reader(reader)?);
                    }
                    Ok(Value::Array(result))
                }
            }
            4 => {
                let len = u32::deserialize_reader(reader)?;
                let mut result = Map::new();
                for _ in 0..len {
                    let key = String::deserialize_reader(reader)?;
                    let value = Value::deserialize_reader(reader)?;
                    result.insert(key, value);
                }
                Ok(Value::Object(result))
            }
            5 => Ok(Value::Null),
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("Invalid Value representation: {}", order),
            )),
        }
    }
}
