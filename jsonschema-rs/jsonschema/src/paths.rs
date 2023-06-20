//! Facilities for working with paths within schemas or validated instances.
use std::{fmt, fmt::Write, slice::Iter, str::FromStr};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
/// JSON Pointer as a wrapper around individual path components.
pub struct JSONPointer(Vec<PathChunk>);

impl JSONPointer {
    /// JSON pointer as a vector of strings. Each component is casted to `String`. Consumes `JSONPointer`.
    #[must_use]
    pub fn into_vec(self) -> Vec<String> {
        self.0
            .into_iter()
            .map(|item| match item {
                PathChunk::Property(value) => value.into_string(),
                PathChunk::Index(idx) => idx.to_string(),
                PathChunk::Keyword(keyword) => keyword.to_string(),
            })
            .collect()
    }

    /// Return an iterator over the underlying vector of path components.
    pub fn iter(&self) -> Iter<'_, PathChunk> {
        self.0.iter()
    }
    /// Take the last pointer chunk.
    #[must_use]
    #[inline]
    pub fn last(&self) -> Option<&PathChunk> {
        self.0.last()
    }

    pub(crate) fn clone_with(&self, chunk: impl Into<PathChunk>) -> Self {
        let mut new = self.clone();
        new.0.push(chunk.into());
        new
    }

    pub(crate) fn extend_with(&self, chunks: &[PathChunk]) -> Self {
        let mut new = self.clone();
        new.0.extend_from_slice(chunks);
        new
    }

    pub(crate) fn as_slice(&self) -> &[PathChunk] {
        &self.0
    }
}

impl serde::Serialize for JSONPointer {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_str(self)
    }
}

impl fmt::Display for JSONPointer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.0.is_empty() {
            for chunk in &self.0 {
                f.write_char('/')?;
                match chunk {
                    PathChunk::Property(value) => {
                        for ch in value.chars() {
                            match ch {
                                '/' => f.write_str("~1")?,
                                '~' => f.write_str("~0")?,
                                _ => f.write_char(ch)?,
                            }
                        }
                    }
                    PathChunk::Index(idx) => f.write_str(itoa::Buffer::new().format(*idx))?,
                    PathChunk::Keyword(keyword) => f.write_str(keyword)?,
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// A key within a JSON object or an index within a JSON array.
/// A sequence of chunks represents a valid path within a JSON value.
///
/// Example:
/// ```json
/// {
///    "cmd": ["ls", "-lh", "/home"]
/// }
/// ```
///
/// To extract "/home" from the JSON object above, we need to take two steps:
/// 1. Go into property "cmd". It corresponds to `PathChunk::Property("cmd".to_string())`.
/// 2. Take the 2nd value from the array - `PathChunk::Index(2)`
///
/// The primary purpose of this enum is to avoid converting indexes to strings during validation.
pub enum PathChunk {
    /// Property name within a JSON object.
    Property(Box<str>),
    /// Index within a JSON array.
    Index(usize),
    /// JSON Schema keyword.
    Keyword(&'static str),
}

#[derive(Debug, Clone)]
pub(crate) struct InstancePath<'a> {
    pub(crate) chunk: Option<PathChunk>,
    pub(crate) parent: Option<&'a InstancePath<'a>>,
}

impl<'a> InstancePath<'a> {
    pub(crate) const fn new() -> Self {
        InstancePath {
            chunk: None,
            parent: None,
        }
    }

    #[inline]
    pub(crate) fn push(&'a self, chunk: impl Into<PathChunk>) -> Self {
        InstancePath {
            chunk: Some(chunk.into()),
            parent: Some(self),
        }
    }

    pub(crate) fn to_vec(&'a self) -> Vec<PathChunk> {
        // The path capacity should be the average depth so we avoid extra allocations
        let mut result = Vec::with_capacity(6);
        let mut current = self;
        if let Some(chunk) = &current.chunk {
            result.push(chunk.clone())
        }
        while let Some(next) = current.parent {
            current = next;
            if let Some(chunk) = &current.chunk {
                result.push(chunk.clone())
            }
        }
        result.reverse();
        result
    }
}

impl IntoIterator for JSONPointer {
    type Item = PathChunk;
    type IntoIter = <Vec<PathChunk> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a JSONPointer {
    type Item = &'a PathChunk;
    type IntoIter = Iter<'a, PathChunk>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl From<String> for PathChunk {
    #[inline]
    fn from(value: String) -> Self {
        PathChunk::Property(value.into_boxed_str())
    }
}
impl From<&'static str> for PathChunk {
    #[inline]
    fn from(value: &'static str) -> Self {
        PathChunk::Keyword(value)
    }
}
impl From<usize> for PathChunk {
    #[inline]
    fn from(value: usize) -> Self {
        PathChunk::Index(value)
    }
}

impl<'a> From<&'a InstancePath<'a>> for JSONPointer {
    #[inline]
    fn from(path: &'a InstancePath<'a>) -> Self {
        JSONPointer(path.to_vec())
    }
}

impl From<InstancePath<'_>> for JSONPointer {
    #[inline]
    fn from(path: InstancePath<'_>) -> Self {
        JSONPointer(path.to_vec())
    }
}

impl From<&[&str]> for JSONPointer {
    #[inline]
    fn from(path: &[&str]) -> Self {
        JSONPointer(
            path.iter()
                .map(|item| PathChunk::Property((*item).into()))
                .collect(),
        )
    }
}
impl From<&[PathChunk]> for JSONPointer {
    #[inline]
    fn from(path: &[PathChunk]) -> Self {
        JSONPointer(path.to_vec())
    }
}

impl From<&str> for JSONPointer {
    fn from(value: &str) -> Self {
        JSONPointer(vec![value.to_string().into()])
    }
}

/// An absolute reference
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbsolutePath(url::Url);

impl AbsolutePath {
    pub(crate) fn with_path(&self, path: &str) -> Self {
        let mut result = self.0.clone();
        result.set_path(path);
        AbsolutePath(result)
    }
}

impl serde::Serialize for AbsolutePath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.0.to_string().as_str())
    }
}

impl FromStr for AbsolutePath {
    type Err = url::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        url::Url::parse(s).map(AbsolutePath::from)
    }
}

impl fmt::Display for AbsolutePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl From<AbsolutePath> for url::Url {
    fn from(p: AbsolutePath) -> Self {
        p.0
    }
}

impl From<url::Url> for AbsolutePath {
    fn from(u: url::Url) -> Self {
        AbsolutePath(u)
    }
}

#[cfg(test)]
mod tests {
    use super::JSONPointer;
    use serde_json::json;

    #[test]
    fn json_pointer_to_string() {
        let chunks = ["/", "~"];
        let pointer = JSONPointer::from(&chunks[..]).to_string();
        assert_eq!(pointer, "/~1/~0");
        let data = json!({"/": {"~": 42}});
        assert_eq!(data.pointer(&pointer), Some(&json!(42)))
    }
}
