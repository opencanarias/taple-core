use num_cmp::NumCmp;
use serde_json::{Map, Value};

use crate::{
    compilation::context::CompilationContext, paths::JSONPointer, primitive_type::PrimitiveType,
    ValidationError,
};

macro_rules! num_cmp {
    ($left:expr, $right:expr) => {
        if let Some(b) = $right.as_u64() {
            NumCmp::num_eq($left, b)
        } else if let Some(b) = $right.as_i64() {
            NumCmp::num_eq($left, b)
        } else {
            NumCmp::num_eq($left, $right.as_f64().expect("Always valid"))
        }
    };
}

#[inline]
pub(crate) fn equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::String(left), Value::String(right)) => left == right,
        (Value::Bool(left), Value::Bool(right)) => left == right,
        (Value::Null, Value::Null) => true,
        (Value::Number(left), Value::Number(right)) => {
            if let Some(a) = left.as_u64() {
                num_cmp!(a, right)
            } else if let Some(a) = left.as_i64() {
                num_cmp!(a, right)
            } else {
                let a = left.as_f64().expect("Always valid");
                num_cmp!(a, right)
            }
        }
        (Value::Array(left), Value::Array(right)) => equal_arrays(left, right),
        (Value::Object(left), Value::Object(right)) => equal_objects(left, right),
        (_, _) => false,
    }
}

#[inline]
pub(crate) fn equal_arrays(left: &[Value], right: &[Value]) -> bool {
    left.len() == right.len() && {
        let mut idx = 0_usize;
        while idx < left.len() {
            if !equal(&left[idx], &right[idx]) {
                return false;
            }
            idx += 1;
        }
        true
    }
}

#[inline]
pub(crate) fn equal_objects(left: &Map<String, Value>, right: &Map<String, Value>) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|((ka, va), (kb, vb))| ka == kb && equal(va, vb))
}

#[inline]
pub(crate) fn map_get_u64<'a>(
    m: &'a Map<String, Value>,
    context: &CompilationContext,
    type_name: &str,
) -> Option<Result<u64, ValidationError<'a>>> {
    let value = m.get(type_name)?;
    match value.as_u64() {
        Some(n) => Some(Ok(n)),
        None if value.is_i64() => Some(Err(ValidationError::minimum(
            JSONPointer::default(),
            context.clone().into_pointer(),
            value,
            0.into(),
        ))),
        None => Some(Err(ValidationError::single_type_error(
            JSONPointer::default(),
            context.clone().into_pointer(),
            value,
            PrimitiveType::Integer,
        ))),
    }
}

/// Fail if the input value is not `u64`.
pub(crate) fn fail_on_non_positive_integer(
    value: &Value,
    instance_path: JSONPointer,
) -> ValidationError<'_> {
    if value.is_i64() {
        ValidationError::minimum(JSONPointer::default(), instance_path, value, 0.into())
    } else {
        ValidationError::single_type_error(
            JSONPointer::default(),
            instance_path,
            value,
            PrimitiveType::Integer,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::equal;
    use serde_json::{json, Value};
    use test_case::test_case;

    #[test_case(&json!(1), &json!(1.0))]
    #[test_case(&json!([2]), &json!([2.0]))]
    #[test_case(&json!([-3]), &json!([-3.0]))]
    #[test_case(&json!({"a": 1}), &json!({"a": 1.0}))]
    fn are_equal(left: &Value, right: &Value) {
        assert!(equal(left, right))
    }

    #[test_case(&json!(1), &json!(2.0))]
    #[test_case(&json!([]), &json!(["foo"]))]
    #[test_case(&json!([-3]), &json!([-4.0]))]
    #[test_case(&json!({"a": 1}), &json!({"a": 1.0, "b": 2}))]
    fn are_not_equal(left: &Value, right: &Value) {
        assert!(!equal(left, right))
    }
}
