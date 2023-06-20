use crate::{
    compilation::context::CompilationContext,
    error::{error, no_error, ErrorIterator, ValidationError},
    keywords::{helpers::equal, CompilationResult},
    validator::Validate,
};
use ahash::{AHashSet, AHasher};
use serde_json::{Map, Value};

use crate::paths::{InstancePath, JSONPointer};
use std::hash::{Hash, Hasher};

// Based on implementation proposed by Sven Marnach:
// https://stackoverflow.com/questions/60882381/what-is-the-fastest-correct-way-to-detect-that-there-are-no-duplicates-in-a-json
#[derive(PartialEq)]
pub(crate) struct HashedValue<'a>(&'a Value);

impl Eq for HashedValue<'_> {}

impl Hash for HashedValue<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self.0 {
            Value::Null => state.write_u32(3_221_225_473), // chosen randomly
            Value::Bool(ref item) => item.hash(state),
            Value::Number(ref item) => {
                if let Some(number) = item.as_u64() {
                    number.hash(state);
                } else if let Some(number) = item.as_i64() {
                    number.hash(state);
                } else if let Some(number) = item.as_f64() {
                    number.to_bits().hash(state)
                }
            }
            Value::String(ref item) => item.hash(state),
            Value::Array(ref items) => {
                for item in items {
                    HashedValue(item).hash(state);
                }
            }
            Value::Object(ref items) => {
                let mut hash = 0;
                for (key, value) in items {
                    // We have no way of building a new hasher of type `H`, so we
                    // hardcode using the default hasher of a hash map.
                    let mut item_hasher = AHasher::default();
                    key.hash(&mut item_hasher);
                    HashedValue(value).hash(&mut item_hasher);
                    hash ^= item_hasher.finish();
                }
                state.write_u64(hash);
            }
        }
    }
}

// Empirically calculated threshold after which the validator resorts to hashing.
// Calculated for an array of mixed types, large homogenous arrays of primitive values might be
// processed faster with different thresholds, but this one gives a good baseline for the common
// case.
const ITEMS_SIZE_THRESHOLD: usize = 15;

#[inline]
pub(crate) fn is_unique(items: &[Value]) -> bool {
    let size = items.len();
    if size <= 1 {
        // Empty arrays and one-element arrays always contain unique elements
        true
    } else if let [first, second] = items {
        !equal(first, second)
    } else if let [first, second, third] = items {
        !equal(first, second) && !equal(first, third) && !equal(second, third)
    } else if size <= ITEMS_SIZE_THRESHOLD {
        // If the array size is small enough we can compare all elements pairwise, which will
        // be faster than calculating hashes for each element, even if the algorithm is O(N^2)
        let mut idx = 0_usize;
        while idx < items.len() {
            let mut inner_idx = idx + 1;
            while inner_idx < items.len() {
                if equal(&items[idx], &items[inner_idx]) {
                    return false;
                }
                inner_idx += 1;
            }
            idx += 1;
        }
        true
    } else {
        let mut seen = AHashSet::with_capacity(size);
        items.iter().map(HashedValue).all(move |x| seen.insert(x))
    }
}

pub(crate) struct UniqueItemsValidator {
    schema_path: JSONPointer,
}

impl UniqueItemsValidator {
    #[inline]
    pub(crate) fn compile<'a>(schema_path: JSONPointer) -> CompilationResult<'a> {
        Ok(Box::new(UniqueItemsValidator { schema_path }))
    }
}

impl Validate for UniqueItemsValidator {
    fn is_valid(&self, instance: &Value) -> bool {
        if let Value::Array(items) = instance {
            if !is_unique(items) {
                return false;
            }
        }
        true
    }

    fn validate<'instance>(
        &self,
        instance: &'instance Value,
        instance_path: &InstancePath,
    ) -> ErrorIterator<'instance> {
        if self.is_valid(instance) {
            no_error()
        } else {
            error(ValidationError::unique_items(
                self.schema_path.clone(),
                instance_path.into(),
                instance,
            ))
        }
    }
}

impl core::fmt::Display for UniqueItemsValidator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        "uniqueItems: true".fmt(f)
    }
}
#[inline]
pub(crate) fn compile<'a>(
    _: &'a Map<String, Value>,
    schema: &'a Value,
    context: &CompilationContext,
) -> Option<CompilationResult<'a>> {
    if let Value::Bool(value) = schema {
        if *value {
            let schema_path = context.as_pointer_with("uniqueItems");
            Some(UniqueItemsValidator::compile(schema_path))
        } else {
            None
        }
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use crate::tests_util;
    use serde_json::json;

    #[test]
    fn schema_path() {
        tests_util::assert_schema_path(
            &json!({"uniqueItems": true}),
            &json!([1, 1]),
            "/uniqueItems",
        )
    }
}
