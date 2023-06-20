use crate::de::ParserNumber;
use crate::error::Error;
use core::fmt::{self, Debug, Display};
use core::hash::{Hash, Hasher};
use borsh::{BorshDeserialize, BorshSerialize};
use serde::de::{self, Unexpected, Visitor};
use serde::{forward_to_deserialize_any, Deserialize, Deserializer, Serialize, Serializer};

/// Represents a JSON number, whether integer or floating point.
#[derive(Clone, PartialEq, Eq, Hash, BorshDeserialize, BorshSerialize)]
pub struct Number {
    n: N,
}

#[derive(Copy, Clone, BorshDeserialize, BorshSerialize)]
enum N {
    PosInt(u64),
    /// Always less than zero.
    NegInt(i64),
    /// Always finite.
    Float(f64),
}

impl PartialEq for N {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (N::PosInt(a), N::PosInt(b)) => a == b,
            (N::NegInt(a), N::NegInt(b)) => a == b,
            (N::Float(a), N::Float(b)) => a == b,
            _ => false,
        }
    }
}

// Implementing Eq is fine since any float values are always finite.
impl Eq for N {}

impl Hash for N {
    fn hash<H: Hasher>(&self, h: &mut H) {
        match *self {
            N::PosInt(i) => i.hash(h),
            N::NegInt(i) => i.hash(h),
            N::Float(f) => {
                if f == 0.0f64 {
                    // There are 2 zero representations, +0 and -0, which
                    // compare equal but have different bits. We use the +0 hash
                    // for both so that hash(+0) == hash(-0).
                    0.0f64.to_bits().hash(h);
                } else {
                    f.to_bits().hash(h);
                }
            }
        }
    }
}

impl Number {
    /// Returns true if the `Number` is an integer between `i64::MIN` and
    /// `i64::MAX`.
    ///
    /// For any Number on which `is_i64` returns true, `as_i64` is guaranteed to
    /// return the integer value.
    ///
    /// ```
    /// # use serde_json::json;
    /// #
    /// let big = i64::max_value() as u64 + 10;
    /// let v = json!({ "a": 64, "b": big, "c": 256.0 });
    ///
    /// assert!(v["a"].is_i64());
    ///
    /// // Greater than i64::MAX.
    /// assert!(!v["b"].is_i64());
    ///
    /// // Numbers with a decimal point are not considered integers.
    /// assert!(!v["c"].is_i64());
    /// ```
    #[inline]
    pub fn is_i64(&self) -> bool {
        match self.n {
            N::PosInt(v) => v <= i64::max_value() as u64,
            N::NegInt(_) => true,
            N::Float(_) => false,
        }
    }

    /// Returns true if the `Number` is an integer between zero and `u64::MAX`.
    ///
    /// For any Number on which `is_u64` returns true, `as_u64` is guaranteed to
    /// return the integer value.
    ///
    /// ```
    /// # use serde_json::json;
    /// #
    /// let v = json!({ "a": 64, "b": -64, "c": 256.0 });
    ///
    /// assert!(v["a"].is_u64());
    ///
    /// // Negative integer.
    /// assert!(!v["b"].is_u64());
    ///
    /// // Numbers with a decimal point are not considered integers.
    /// assert!(!v["c"].is_u64());
    /// ```
    #[inline]
    pub fn is_u64(&self) -> bool {
        match self.n {
            N::PosInt(_) => true,
            N::NegInt(_) | N::Float(_) => false,
        }
    }

    /// Returns true if the `Number` can be represented by f64.
    ///
    /// For any Number on which `is_f64` returns true, `as_f64` is guaranteed to
    /// return the floating point value.
    ///
    /// Currently this function returns true if and only if both `is_i64` and
    /// `is_u64` return false but this is not a guarantee in the future.
    ///
    /// ```
    /// # use serde_json::json;
    /// #
    /// let v = json!({ "a": 256.0, "b": 64, "c": -64 });
    ///
    /// assert!(v["a"].is_f64());
    ///
    /// // Integers.
    /// assert!(!v["b"].is_f64());
    /// assert!(!v["c"].is_f64());
    /// ```
    #[inline]
    pub fn is_f64(&self) -> bool {
        match self.n {
            N::Float(_) => true,
            N::PosInt(_) | N::NegInt(_) => false,
        }
    }

    /// If the `Number` is an integer, represent it as i64 if possible. Returns
    /// None otherwise.
    ///
    /// ```
    /// # use serde_json::json;
    /// #
    /// let big = i64::max_value() as u64 + 10;
    /// let v = json!({ "a": 64, "b": big, "c": 256.0 });
    ///
    /// assert_eq!(v["a"].as_i64(), Some(64));
    /// assert_eq!(v["b"].as_i64(), None);
    /// assert_eq!(v["c"].as_i64(), None);
    /// ```
    #[inline]
    pub fn as_i64(&self) -> Option<i64> {
        #[cfg(not(feature = "arbitrary_precision"))]
        match self.n {
            N::PosInt(n) => {
                if n <= i64::max_value() as u64 {
                    Some(n as i64)
                } else {
                    None
                }
            }
            N::NegInt(n) => Some(n),
            N::Float(_) => None,
        }
        #[cfg(feature = "arbitrary_precision")]
        self.n.parse().ok()
    }

    /// If the `Number` is an integer, represent it as u64 if possible. Returns
    /// None otherwise.
    ///
    /// ```
    /// # use serde_json::json;
    /// #
    /// let v = json!({ "a": 64, "b": -64, "c": 256.0 });
    ///
    /// assert_eq!(v["a"].as_u64(), Some(64));
    /// assert_eq!(v["b"].as_u64(), None);
    /// assert_eq!(v["c"].as_u64(), None);
    /// ```
    #[inline]
    pub fn as_u64(&self) -> Option<u64> {
        #[cfg(not(feature = "arbitrary_precision"))]
        match self.n {
            N::PosInt(n) => Some(n),
            N::NegInt(_) | N::Float(_) => None,
        }
        #[cfg(feature = "arbitrary_precision")]
        self.n.parse().ok()
    }

    /// Represents the number as f64 if possible. Returns None otherwise.
    ///
    /// ```
    /// # use serde_json::json;
    /// #
    /// let v = json!({ "a": 256.0, "b": 64, "c": -64 });
    ///
    /// assert_eq!(v["a"].as_f64(), Some(256.0));
    /// assert_eq!(v["b"].as_f64(), Some(64.0));
    /// assert_eq!(v["c"].as_f64(), Some(-64.0));
    /// ```
    #[inline]
    pub fn as_f64(&self) -> Option<f64> {
        #[cfg(not(feature = "arbitrary_precision"))]
        match self.n {
            N::PosInt(n) => Some(n as f64),
            N::NegInt(n) => Some(n as f64),
            N::Float(n) => Some(n),
        }
        #[cfg(feature = "arbitrary_precision")]
        self.n.parse::<f64>().ok().filter(|float| float.is_finite())
    }

    /// Converts a finite `f64` to a `Number`. Infinite or NaN values are not JSON
    /// numbers.
    ///
    /// ```
    /// # use std::f64;
    /// #
    /// # use serde_json::Number;
    /// #
    /// assert!(Number::from_f64(256.0).is_some());
    ///
    /// assert!(Number::from_f64(f64::NAN).is_none());
    /// ```
    #[inline]
    pub fn from_f64(f: f64) -> Option<Number> {
        if f.is_finite() {
            let n = {
                #[cfg(not(feature = "arbitrary_precision"))]
                {
                    N::Float(f)
                }
                #[cfg(feature = "arbitrary_precision")]
                {
                    ryu::Buffer::new().format_finite(f).to_owned()
                }
            };
            Some(Number { n })
        } else {
            None
        }
    }

    pub(crate) fn as_f32(&self) -> Option<f32> {
        #[cfg(not(feature = "arbitrary_precision"))]
        match self.n {
            N::PosInt(n) => Some(n as f32),
            N::NegInt(n) => Some(n as f32),
            N::Float(n) => Some(n as f32),
        }
        #[cfg(feature = "arbitrary_precision")]
        self.n.parse::<f32>().ok().filter(|float| float.is_finite())
    }

    pub(crate) fn from_f32(f: f32) -> Option<Number> {
        if f.is_finite() {
            let n = {
                #[cfg(not(feature = "arbitrary_precision"))]
                {
                    N::Float(f as f64)
                }
                #[cfg(feature = "arbitrary_precision")]
                {
                    ryu::Buffer::new().format_finite(f).to_owned()
                }
            };
            Some(Number { n })
        } else {
            None
        }
    }

    #[cfg(feature = "arbitrary_precision")]
    /// Not public API. Only tests use this.
    #[doc(hidden)]
    #[inline]
    pub fn from_string_unchecked(n: String) -> Self {
        Number { n }
    }
}

impl Display for Number {
    #[cfg(not(feature = "arbitrary_precision"))]
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        match self.n {
            N::PosInt(u) => formatter.write_str(itoa::Buffer::new().format(u)),
            N::NegInt(i) => formatter.write_str(itoa::Buffer::new().format(i)),
            N::Float(f) => formatter.write_str(ryu::Buffer::new().format_finite(f)),
        }
    }

    #[cfg(feature = "arbitrary_precision")]
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        Display::fmt(&self.n, formatter)
    }
}

impl Debug for Number {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "Number({})", self)
    }
}

impl Serialize for Number {
    #[cfg(not(feature = "arbitrary_precision"))]
    #[inline]
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self.n {
            N::PosInt(u) => serializer.serialize_u64(u),
            N::NegInt(i) => serializer.serialize_i64(i),
            N::Float(f) => serializer.serialize_f64(f),
        }
    }

    #[cfg(feature = "arbitrary_precision")]
    #[inline]
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeStruct;

        let mut s = serializer.serialize_struct(TOKEN, 1)?;
        s.serialize_field(TOKEN, &self.n)?;
        s.end()
    }
}

impl<'de> Deserialize<'de> for Number {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Number, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct NumberVisitor;

        impl<'de> Visitor<'de> for NumberVisitor {
            type Value = Number;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a JSON number")
            }

            #[inline]
            fn visit_i64<E>(self, value: i64) -> Result<Number, E> {
                Ok(value.into())
            }

            #[inline]
            fn visit_u64<E>(self, value: u64) -> Result<Number, E> {
                Ok(value.into())
            }

            #[inline]
            fn visit_f64<E>(self, value: f64) -> Result<Number, E>
            where
                E: de::Error,
            {
                Number::from_f64(value).ok_or_else(|| de::Error::custom("not a JSON number"))
            }

            #[cfg(feature = "arbitrary_precision")]
            #[inline]
            fn visit_map<V>(self, mut visitor: V) -> Result<Number, V::Error>
            where
                V: de::MapAccess<'de>,
            {
                let value = visitor.next_key::<NumberKey>()?;
                if value.is_none() {
                    return Err(de::Error::invalid_type(Unexpected::Map, &self));
                }
                let v: NumberFromString = visitor.next_value()?;
                Ok(v.value)
            }
        }

        deserializer.deserialize_any(NumberVisitor)
    }
}

macro_rules! deserialize_any {
    (@expand [$($num_string:tt)*]) => {
        #[inline]
        fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Error>
        where
            V: Visitor<'de>,
        {
            match self.n {
                N::PosInt(u) => visitor.visit_u64(u),
                N::NegInt(i) => visitor.visit_i64(i),
                N::Float(f) => visitor.visit_f64(f),
            }
        }
    };

    (owned) => {
        deserialize_any!(@expand [n]);
    };

    (ref) => {
        deserialize_any!(@expand [n.clone()]);
    };
}

macro_rules! deserialize_number {
    ($deserialize:ident => $visit:ident) => {
        fn $deserialize<V>(self, visitor: V) -> Result<V::Value, Error>
        where
            V: Visitor<'de>,
        {
            self.deserialize_any(visitor)
        }
    };
}

impl<'de> Deserializer<'de> for Number {
    type Error = Error;

    deserialize_any!(owned);

    deserialize_number!(deserialize_i8 => visit_i8);
    deserialize_number!(deserialize_i16 => visit_i16);
    deserialize_number!(deserialize_i32 => visit_i32);
    deserialize_number!(deserialize_i64 => visit_i64);
    deserialize_number!(deserialize_i128 => visit_i128);
    deserialize_number!(deserialize_u8 => visit_u8);
    deserialize_number!(deserialize_u16 => visit_u16);
    deserialize_number!(deserialize_u32 => visit_u32);
    deserialize_number!(deserialize_u64 => visit_u64);
    deserialize_number!(deserialize_u128 => visit_u128);
    deserialize_number!(deserialize_f32 => visit_f32);
    deserialize_number!(deserialize_f64 => visit_f64);

    forward_to_deserialize_any! {
        bool char str string bytes byte_buf option unit unit_struct
        newtype_struct seq tuple tuple_struct map struct enum identifier
        ignored_any
    }
}

impl<'de, 'a> Deserializer<'de> for &'a Number {
    type Error = Error;

    deserialize_any!(ref);

    deserialize_number!(deserialize_i8 => visit_i8);
    deserialize_number!(deserialize_i16 => visit_i16);
    deserialize_number!(deserialize_i32 => visit_i32);
    deserialize_number!(deserialize_i64 => visit_i64);
    deserialize_number!(deserialize_i128 => visit_i128);
    deserialize_number!(deserialize_u8 => visit_u8);
    deserialize_number!(deserialize_u16 => visit_u16);
    deserialize_number!(deserialize_u32 => visit_u32);
    deserialize_number!(deserialize_u64 => visit_u64);
    deserialize_number!(deserialize_u128 => visit_u128);
    deserialize_number!(deserialize_f32 => visit_f32);
    deserialize_number!(deserialize_f64 => visit_f64);

    forward_to_deserialize_any! {
        bool char str string bytes byte_buf option unit unit_struct
        newtype_struct seq tuple tuple_struct map struct enum identifier
        ignored_any
    }
}

impl From<ParserNumber> for Number {
    fn from(value: ParserNumber) -> Self {
        let n = match value {
            ParserNumber::F64(f) => {
                {
                    N::Float(f)
                }
            }
            ParserNumber::U64(u) => {
                {
                    N::PosInt(u)
                }
            }
            ParserNumber::I64(i) => {
                {
                    N::NegInt(i)
                }
            }
        };
        Number { n }
    }
}

macro_rules! impl_from_unsigned {
    (
        $($ty:ty),*
    ) => {
        $(
            impl From<$ty> for Number {
                #[inline]
                fn from(u: $ty) -> Self {
                    let n = {
                        { N::PosInt(u as u64) }
                    };
                    Number { n }
                }
            }
        )*
    };
}

macro_rules! impl_from_signed {
    (
        $($ty:ty),*
    ) => {
        $(
            impl From<$ty> for Number {
                #[inline]
                fn from(i: $ty) -> Self {
                    let n = {
                        {
                            if i < 0 {
                                N::NegInt(i as i64)
                            } else {
                                N::PosInt(i as u64)
                            }
                        }
                    };
                    Number { n }
                }
            }
        )*
    };
}

impl_from_unsigned!(u8, u16, u32, u64, usize);
impl_from_signed!(i8, i16, i32, i64, isize);

impl Number {
    #[cold]
    pub(crate) fn unexpected(&self) -> Unexpected {
        match self.n {
            N::PosInt(u) => Unexpected::Unsigned(u),
            N::NegInt(i) => Unexpected::Signed(i),
            N::Float(f) => Unexpected::Float(f),
        }
    }
}
