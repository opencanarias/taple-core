use crate::{
    compilation::context::CompilationContext,
    error::{error, no_error, ErrorIterator, ValidationError},
    keywords::CompilationResult,
    paths::{InstancePath, JSONPointer},
    primitive_type::PrimitiveType,
    validator::Validate,
};
use fraction::{BigFraction, BigUint};
use serde_json::{Map, Value};

pub(crate) struct MultipleOfFloatValidator {
    multiple_of: f64,
    schema_path: JSONPointer,
}

impl MultipleOfFloatValidator {
    #[inline]
    pub(crate) fn compile<'a>(multiple_of: f64, schema_path: JSONPointer) -> CompilationResult<'a> {
        Ok(Box::new(MultipleOfFloatValidator {
            multiple_of,
            schema_path,
        }))
    }
}

impl Validate for MultipleOfFloatValidator {
    fn is_valid(&self, instance: &Value) -> bool {
        if let Value::Number(item) = instance {
            let item = item.as_f64().expect("Always valid");
            let remainder = (item / self.multiple_of) % 1.;
            if remainder.is_nan() {
                // Involves heap allocations via the underlying `BigUint` type
                let fraction = BigFraction::from(item) / BigFraction::from(self.multiple_of);
                if let Some(denom) = fraction.denom() {
                    denom == &BigUint::from(1_u8)
                } else {
                    true
                }
            } else {
                remainder < f64::EPSILON
            }
        } else {
            true
        }
    }

    fn validate<'instance>(
        &self,
        instance: &'instance Value,
        instance_path: &InstancePath,
    ) -> ErrorIterator<'instance> {
        if !self.is_valid(instance) {
            return error(ValidationError::multiple_of(
                self.schema_path.clone(),
                instance_path.into(),
                instance,
                self.multiple_of,
            ));
        }
        no_error()
    }
}

impl core::fmt::Display for MultipleOfFloatValidator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "multipleOf: {}", self.multiple_of)
    }
}

pub(crate) struct MultipleOfIntegerValidator {
    multiple_of: f64,
    schema_path: JSONPointer,
}

impl MultipleOfIntegerValidator {
    #[inline]
    pub(crate) fn compile<'a>(multiple_of: f64, schema_path: JSONPointer) -> CompilationResult<'a> {
        Ok(Box::new(MultipleOfIntegerValidator {
            multiple_of,
            schema_path,
        }))
    }
}

impl Validate for MultipleOfIntegerValidator {
    fn is_valid(&self, instance: &Value) -> bool {
        if let Value::Number(item) = instance {
            let item = item.as_f64().expect("Always valid");
            // As the divisor has its fractional part as zero, then any value with a non-zero
            // fractional part can't be a multiple of this divisor, therefore it is short-circuited
            item.fract() == 0. && (item % self.multiple_of) == 0.
        } else {
            true
        }
    }

    fn validate<'instance>(
        &self,
        instance: &'instance Value,
        instance_path: &InstancePath,
    ) -> ErrorIterator<'instance> {
        if !self.is_valid(instance) {
            return error(ValidationError::multiple_of(
                self.schema_path.clone(),
                instance_path.into(),
                instance,
                self.multiple_of,
            ));
        }
        no_error()
    }
}

impl core::fmt::Display for MultipleOfIntegerValidator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "multipleOf: {}", self.multiple_of)
    }
}
#[inline]
pub(crate) fn compile<'a>(
    _: &'a Map<String, Value>,
    schema: &'a Value,
    context: &CompilationContext,
) -> Option<CompilationResult<'a>> {
    if let Value::Number(multiple_of) = schema {
        let multiple_of = multiple_of.as_f64().expect("Always valid");
        let schema_path = context.as_pointer_with("multipleOf");
        if multiple_of.fract() == 0. {
            Some(MultipleOfIntegerValidator::compile(
                multiple_of,
                schema_path,
            ))
        } else {
            Some(MultipleOfFloatValidator::compile(multiple_of, schema_path))
        }
    } else {
        Some(Err(ValidationError::single_type_error(
            JSONPointer::default(),
            context.clone().into_pointer(),
            schema,
            PrimitiveType::Number,
        )))
    }
}

#[cfg(test)]
mod tests {
    use crate::tests_util;
    use serde_json::{json, Value};
    use test_case::test_case;

    #[test_case(&json!({"multipleOf": 2}), &json!(4))]
    #[test_case(&json!({"multipleOf": 1.0}), &json!(4.0))]
    #[test_case(&json!({"multipleOf": 1.5}), &json!(3.0))]
    #[test_case(&json!({"multipleOf": 1.5}), &json!(4.5))]
    fn multiple_of_is_valid(schema: &Value, instance: &Value) {
        tests_util::is_valid(schema, instance)
    }

    #[test_case(&json!({"multipleOf": 1.0}), &json!(4.5))]
    fn multiple_of_is_not_valid(schema: &Value, instance: &Value) {
        tests_util::is_not_valid(schema, instance)
    }

    #[test_case(&json!({"multipleOf": 2}), &json!(3), "/multipleOf")]
    #[test_case(&json!({"multipleOf": 1.5}), &json!(5), "/multipleOf")]
    fn schema_path(schema: &Value, instance: &Value, expected: &str) {
        tests_util::assert_schema_path(schema, instance, expected)
    }
}
