use crate::DatabaseCollection;
use crate::DbError;

const MAX_U64: usize = 17; // Max size u64

pub(crate) enum Element {
    N(u64),
    S(String),
}

fn get_u64_as_hexadecimal(value: u64) -> String {
    format!("{:0width$}", format!("{:016x}", value), width = MAX_U64)
}

pub(crate) fn get_key(key_elements: Vec<Element>) -> Result<String, DbError> {
    if key_elements.len() > 0 {
        let mut key: String = String::from("");
        for i in 0..(key_elements.len() - 1) {
            key.push_str(&{
                match &key_elements[i] {
                    Element::N(n) => get_u64_as_hexadecimal(*n),
                    Element::S(s) => s.to_string(),
                }
            });
            key.push_str(&char::MAX.to_string());
        }
        key.push_str(&{
            match &key_elements[key_elements.len() - 1] {
                Element::N(n) => get_u64_as_hexadecimal(*n),
                Element::S(s) => s.to_string(),
            }
        });
        Ok(format!("{}", key))
    } else {
        Err(DbError::KeyElementsError)
    }
}

pub(crate) fn get_by_range<C: DatabaseCollection>(
    from: Option<String>,
    quantity: isize,
    collection: &C,
    prefix: &str,
) -> Result<Vec<Vec<u8>>, DbError> {
    fn convert<'a>(
        iter: impl Iterator<Item = (String, Vec<u8>)> + 'a,
    ) -> Box<dyn Iterator<Item = (String, Vec<u8>)> + 'a> {
        Box::new(iter)
    }

    let (mut iter, quantity) = match from {
        Some(key) => {
            // Get true key
            let key_elements: Vec<Element> = vec![Element::S(prefix.to_string()), Element::S(key)];
            let key = get_key(key_elements)?;
            // Find the key
            let iter = if quantity >= 0 {
                collection.iter(false, format!("{}{}", prefix, char::MAX))
            } else {
                collection.iter(true, format!("{}{}", prefix, char::MAX))
            };
            let mut iter = iter.peekable();
            loop {
                let Some((current_key, _)) = iter.peek() else {
                    return Err(DbError::EntryNotFound);
                };
                if current_key == &key {
                    break;
                }
                iter.next();
            }
            iter.next(); // Exclusive From
            (convert(iter), quantity.abs())
        }
        None => {
            if quantity >= 0 {
                (collection.iter(false, prefix.to_string()), quantity)
            } else {
                (collection.iter(true, prefix.to_string()), quantity.abs())
            }
        }
    };
    let mut result = Vec::new();
    let mut counter = 0;
    while counter < quantity {
        let Some((_, event)) = iter.next() else {
            break;
        };
        result.push(event);
        counter += 1;
    }
    Ok(result)
}
