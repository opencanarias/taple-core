//! Print derivation module
//!

pub mod digest;
pub(crate) mod key;
pub(crate) mod signature;

pub use key::KeyDerivator;
pub use signature::SignatureDerivator;

/// Derivator trait
pub trait Derivator {
    fn code_len(&self) -> usize;
    fn derivative_len(&self) -> usize;
    fn material_len(&self) -> usize {
        self.code_len() + self.derivative_len()
    }
    fn to_str(&self) -> String;
}
