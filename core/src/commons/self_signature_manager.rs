use std::collections::HashSet;

use super::{
    errors::ProtocolErrors,
    models::{timestamp, HashId},
};
use crate::commons::{
    config::TapleSettings,
    crypto::{KeyMaterial, KeyPair, Payload, DSA},
    identifier::{derive::digest::DigestDerivator, KeyIdentifier},
    models::signature::Signature,
};

pub trait SelfSignatureInterface {
    fn change_settings(&mut self, settings: &TapleSettings);
    fn get_own_identifier(&self) -> KeyIdentifier;
    fn sign<T: HashId>(&self, content: &T) -> Result<Signature, ProtocolErrors>;
    fn check_if_signature_present(&self, signers: &HashSet<KeyIdentifier>) -> bool;
}

#[derive(Clone, Debug)]
pub struct SelfSignatureManager {
    pub keys: KeyPair,
    pub identifier: KeyIdentifier,
    pub digest_derivator: DigestDerivator,
}

impl SelfSignatureManager {
    pub fn new(keys: KeyPair, settings: &TapleSettings) -> Self {
        let identifier = KeyIdentifier::new(keys.get_key_derivator(), &keys.public_key_bytes());
        Self {
            keys,
            identifier,
            digest_derivator: settings.node.digest_derivator.clone(),
        }
    }
}

impl SelfSignatureInterface for SelfSignatureManager {
    fn change_settings(&mut self, settings: &TapleSettings) {
        self.digest_derivator = settings.node.digest_derivator.clone();
    }

    fn get_own_identifier(&self) -> KeyIdentifier {
        self.identifier.clone()
    }

    fn sign<T: HashId>(&self, content: &T) -> Result<Signature, ProtocolErrors> {
        Ok(Signature::new(content, self.identifier.clone(), &self.keys)
            .map_err(|_| ProtocolErrors::SignatureError)?)
    }

    fn check_if_signature_present(&self, signers: &HashSet<KeyIdentifier>) -> bool {
        signers.contains(&self.identifier)
    }
}
