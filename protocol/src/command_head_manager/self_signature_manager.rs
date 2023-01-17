use std::collections::HashSet;

use crate::errors::ProtocolErrors;
use borsh::BorshSerialize;
use chrono::Utc;
use commons::{
    config::TapleSettings,
    crypto::{KeyMaterial, KeyPair, Payload, DSA},
    identifier::{
        derive::digest::DigestDerivator, Derivable, DigestIdentifier, KeyIdentifier,
        SignatureIdentifier,
    },
    models::signature::{Signature, SignatureContent},
};

pub trait SelfSignatureInterface {
    fn change_settings(&mut self, settings: &TapleSettings);
    fn get_own_identifier(&self) -> KeyIdentifier;
    fn sign<T: BorshSerialize>(&self, content: &T) -> Result<Signature, ProtocolErrors>;
    fn check_if_signature_present(&self, signers: &HashSet<KeyIdentifier>) -> bool;
}

pub struct SelfSignatureManager {
    keys: KeyPair,
    identifier: KeyIdentifier,
    digest_derivator: DigestDerivator,
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

    fn sign<T: BorshSerialize>(&self, content: &T) -> Result<Signature, ProtocolErrors> {
        let hash = DigestIdentifier::from_serializable_borsh(content).expect("Serialización falla");
        let signature = self
            .keys
            .sign(Payload::Buffer(hash.derivative()))
            .map_err(|_| ProtocolErrors::SignatureError)?;
        Ok(Signature {
            content: SignatureContent {
                signer: self.identifier.clone(),
                event_content_hash: hash,
                timestamp: Utc::now().timestamp_millis(),
            },
            signature: SignatureIdentifier::new(
                self.identifier.to_signature_derivator(),
                &signature,
            ),
        })
    }

    fn check_if_signature_present(&self, signers: &HashSet<KeyIdentifier>) -> bool {
        signers.contains(&self.identifier)
    }
}
