use std::collections::HashSet;
use std::sync::Arc;

use crate::commons::models::event::ValidationProof;
use crate::commons::models::notary::NotaryEventResponse;
use crate::commons::models::state::Subject;
use crate::crypto::KeyPair;
use crate::event_request::EventRequest;
use crate::identifier::{DigestIdentifier, KeyIdentifier};
use crate::signature::Signature;
use crate::Event;

use super::error::Error;
use super::layers::lce_validation_proofs::{self, LceValidationProofs};
use super::{
    layers::{
        contract::ContractDb, controller_id::ControllerIdDb, event::EventDb, notary::NotaryDb,
        preauthorized_subjects_and_providers::PreauthorizedSbujectsAndProovidersDb,
        prevalidated_event::PrevalidatedEventDb, request::RequestDb, signature::SignatureDb,
        subject::SubjectDb, subject_by_governance::SubjectByGovernanceDb,
        transfer_events::TransferEventsDb, witness_signatures::WitnessSignaturesDb,
    },
    DatabaseCollection, DatabaseManager,
};

pub struct DB<C: DatabaseCollection> {
    signature_db: SignatureDb<C>,
    subject_db: SubjectDb<C>,
    event_db: EventDb<C>,
    prevalidated_event_db: PrevalidatedEventDb<C>,
    request_db: RequestDb<C>,
    controller_id_db: ControllerIdDb<C>,
    notary_db: NotaryDb<C>,
    contract_db: ContractDb<C>,
    witness_signatures_db: WitnessSignaturesDb<C>,
    subject_by_governance_db: SubjectByGovernanceDb<C>,
    transfer_events_db: TransferEventsDb<C>,
    preauthorized_subjects_and_providers_db: PreauthorizedSbujectsAndProovidersDb<C>,
    lce_validation_proofs_db: LceValidationProofs<C>,
}

impl<C: DatabaseCollection> DB<C> {
    pub fn new<M: DatabaseManager<C>>(manager: Arc<M>) -> Self {
        let signature_db = SignatureDb::new(&manager);
        let subject_db = SubjectDb::new(&manager);
        let event_db = EventDb::new(&manager);
        let prevalidated_event_db = PrevalidatedEventDb::new(&manager);
        let request_db = RequestDb::new(&manager);
        let controller_id_db = ControllerIdDb::new(&manager);
        let notary_db = NotaryDb::new(&manager);
        let contract_db = ContractDb::new(&manager);
        let witness_signatures_db = WitnessSignaturesDb::new(&manager);
        let subject_by_governance_db = SubjectByGovernanceDb::new(&manager);
        let transfer_events_db = TransferEventsDb::new(&manager);
        let preauthorized_subjects_and_providers_db =
            PreauthorizedSbujectsAndProovidersDb::new(&manager);
        let lce_validation_proofs_db = LceValidationProofs::new(&manager);
        Self {
            signature_db,
            subject_db,
            event_db,
            prevalidated_event_db,
            request_db,
            controller_id_db,
            notary_db,
            contract_db,
            witness_signatures_db,
            subject_by_governance_db,
            transfer_events_db,
            preauthorized_subjects_and_providers_db,
            lce_validation_proofs_db,
        }
    }

    pub fn get_signatures(
        &self,
        subject_id: &DigestIdentifier,
        sn: u64,
    ) -> Result<(HashSet<Signature>, ValidationProof), Error> {
        self.signature_db.get_signatures(subject_id, sn)
    }

    pub fn set_signatures(
        &self,
        subject_id: &DigestIdentifier,
        sn: u64,
        signatures: HashSet<Signature>,
        validation_proof: ValidationProof,
    ) -> Result<(), Error> {
        self.signature_db
            .set_signatures(subject_id, sn, signatures, validation_proof)
    }

    pub fn del_signatures(&self, subject_id: &DigestIdentifier, sn: u64) -> Result<(), Error> {
        self.signature_db.del_signatures(subject_id, sn)
    }

    pub fn get_validation_proof(&self, subject_id: &DigestIdentifier) -> Result<HashSet<Signature>, Error> {
        self.signature_db.get_validation_proof(subject_id)
    }

    pub fn get_subject(&self, subject_id: &DigestIdentifier) -> Result<Subject, Error> {
        self.subject_db.get_subject(subject_id)
    }

    pub fn set_subject(
        &self,
        subject_id: &DigestIdentifier,
        subject: Subject,
    ) -> Result<(), Error> {
        self.subject_db.set_subject(subject_id, subject)
    }

    pub fn get_subjects(
        &self,
        from: Option<String>,
        quantity: isize,
    ) -> Result<Vec<Subject>, Error> {
        self.subject_db.get_subjects(from, quantity)
    }

    pub fn del_subject(&self, subject_id: &DigestIdentifier) -> Result<(), Error> {
        self.subject_db.del_subject(subject_id)
    }

    pub fn get_all_subjects(&self) -> Vec<Subject> {
        self.subject_db.get_all_subjects()
    }

    pub fn get_event(&self, subject_id: &DigestIdentifier, sn: u64) -> Result<Event, Error> {
        self.event_db.get_event(subject_id, sn)
    }

    pub fn get_events_by_range(
        &self,
        subject_id: &DigestIdentifier,
        from: Option<i64>,
        quantity: isize,
    ) -> Result<Vec<Event>, Error> {
        self.event_db
            .get_events_by_range(subject_id, from, quantity)
    }

    pub fn set_event(&self, subject_id: &DigestIdentifier, event: Event) -> Result<(), Error> {
        self.event_db.set_event(subject_id, event)
    }

    pub fn del_event(&self, subject_id: &DigestIdentifier, sn: u64) -> Result<(), Error> {
        self.event_db.del_event(subject_id, sn)
    }

    pub fn get_prevalidated_event(&self, subject_id: &DigestIdentifier) -> Result<Event, Error> {
        self.prevalidated_event_db
            .get_prevalidated_event(subject_id)
    }

    pub fn set_prevalidated_event(
        &self,
        subject_id: &DigestIdentifier,
        event: Event,
    ) -> Result<(), Error> {
        self.prevalidated_event_db
            .set_prevalidated_event(subject_id, event)
    }

    pub fn del_prevalidated_event(&self, subject_id: &DigestIdentifier) -> Result<(), Error> {
        self.prevalidated_event_db
            .del_prevalidated_event(subject_id)
    }

    pub fn get_request(&self, subject_id: &DigestIdentifier) -> Result<EventRequest, Error> {
        self.request_db.get_request(subject_id)
    }

    pub fn get_all_request(&self) -> Vec<EventRequest> {
        self.request_db.get_all_request()
    }

    pub fn set_request(
        &self,
        subject_id: &DigestIdentifier,
        request: EventRequest,
    ) -> Result<(), Error> {
        self.request_db.set_request(subject_id, request)
    }

    pub fn del_request(&self, subject_id: &DigestIdentifier) -> Result<(), Error> {
        self.request_db.del_request(subject_id)
    }

    pub fn get_controller_id(&self) -> Result<String, Error> {
        self.controller_id_db.get_controller_id()
    }

    pub fn set_controller_id(&self, controller_id: String) -> Result<(), Error> {
        self.controller_id_db.set_controller_id(controller_id)
    }

    pub fn get_notary_register(
        &self,
        subject_id: &DigestIdentifier,
    ) -> Result<ValidationProof, Error> {
        self.notary_db.get_notary_register(subject_id)
    }

    pub fn set_notary_register(
        &self,
        subject_id: &DigestIdentifier,
        validation_proof: &ValidationProof,
    ) -> Result<(), Error> {
        self.notary_db
            .set_notary_register(subject_id, validation_proof)
    }

    pub fn get_contract(
        &self,
        governance_id: &DigestIdentifier,
        schema_id: &str,
    ) -> Result<(Vec<u8>, DigestIdentifier, u64), Error> {
        self.contract_db.get_contract(governance_id, schema_id)
    }

    pub fn put_contract(
        &self,
        governance_id: &DigestIdentifier,
        schema_id: &str,
        contract: Vec<u8>,
        hash: DigestIdentifier,
        gov_version: u64,
    ) -> Result<(), Error> {
        self.contract_db
            .put_contract(governance_id, schema_id, contract, hash, gov_version)
    }

    pub fn get_governance_contract(&self) -> Result<Vec<u8>, Error> {
        self.contract_db.get_governance_contract()
    }

    pub fn put_governance_contract(&self, contract: Vec<u8>) -> Result<(), Error> {
        self.contract_db.put_governance_contract(contract)
    }

    pub fn get_witness_signatures(
        &self,
        subject_id: &DigestIdentifier,
    ) -> Result<(u64, HashSet<Signature>), Error> {
        self.witness_signatures_db
            .get_witness_signatures(subject_id)
    }

    pub fn get_all_witness_signatures(
        &self,
    ) -> Result<Vec<(DigestIdentifier, u64, HashSet<Signature>)>, Error> {
        self.witness_signatures_db.get_all_witness_signatures()
    }

    pub fn set_witness_signatures(
        &self,
        subject_id: &DigestIdentifier,
        sn: u64,
        signatures: HashSet<Signature>,
    ) -> Result<(), Error> {
        self.witness_signatures_db
            .set_witness_signatures(subject_id, sn, signatures)
    }

    pub fn del_witness_signatures(&self, subject_id: &DigestIdentifier) -> Result<(), Error> {
        self.witness_signatures_db
            .del_witness_signatures(subject_id)
    }

    pub fn set_governance_index(
        &self,
        subject_id: &DigestIdentifier,
        governance_id: &DigestIdentifier,
    ) -> Result<(), Error> {
        self.subject_by_governance_db
            .set_governance_index(subject_id, governance_id)
    }

    pub fn get_subjects_by_governance(
        &self,
        governance_id: &DigestIdentifier,
    ) -> Result<Vec<DigestIdentifier>, Error> {
        self.subject_by_governance_db
            .get_subjects_by_governance(governance_id)
    }

    pub fn get_governances(
        &self,
        from: Option<String>,
        quantity: isize,
    ) -> Result<Vec<Subject>, Error> {
        self.subject_by_governance_db.get_governances(from, quantity)
    }

    pub fn get_governance_subjects(
        &self,
        governance_id: &DigestIdentifier,
        from: Option<String>,
        quantity: isize,
    ) -> Result<Vec<Subject>, Error> {
        self.subject_by_governance_db
            .get_governance_subjects(governance_id, from, quantity)
    }

    pub fn get_expecting_transfer(&self, subject_id: &DigestIdentifier) -> Result<KeyPair, Error> {
        self.transfer_events_db.get_expecting_transfer(subject_id)
    }

    pub fn get_all_expecting_transfers(
        &self,
    ) -> Result<Vec<(DigestIdentifier, HashSet<KeyIdentifier>)>, Error> {
        self.transfer_events_db.get_all_expecting_transfers()
    }

    pub fn set_expecting_transfer(
        &self,
        subject_id: &DigestIdentifier,
        keypair: KeyPair,
    ) -> Result<(), Error> {
        self.transfer_events_db
            .set_expecting_transfer(subject_id, keypair)
    }

    pub fn del_expecting_transfer(&self, subject_id: &DigestIdentifier) -> Result<(), Error> {
        self.transfer_events_db.del_expecting_transfer(subject_id)
    }

    pub fn get_preauthorized_subject_and_providers(
        &self,
        subject_id: &DigestIdentifier,
    ) -> Result<HashSet<KeyIdentifier>, Error> {
        self.preauthorized_subjects_and_providers_db
            .get_preauthorized_subject_and_providers(subject_id)
    }

    pub fn get_preauthorized_subjects_and_providers(
        &self,
        from: Option<String>,
        quantity: isize,
    ) -> Result<Vec<(DigestIdentifier, HashSet<KeyIdentifier>)>, Error> {
        self.preauthorized_subjects_and_providers_db
            .get_preauthorized_subjects_and_providers(from, quantity)
    }

    pub fn set_preauthorized_subject_and_providers(
        &self,
        subject_id: &DigestIdentifier,
        providers: HashSet<KeyIdentifier>,
    ) -> Result<(), Error> {
        self.preauthorized_subjects_and_providers_db
            .set_preauthorized_subject_and_providers(subject_id, providers)
    }

    pub fn get_lce_validation_proof(
        &self,
        subject_id: &DigestIdentifier,
    ) -> Result<ValidationProof, Error> {
        self.lce_validation_proofs_db
            .get_lce_validation_proof(subject_id)
    }

    pub fn set_lce_validation_proof(
        &self,
        subject_id: &DigestIdentifier,
        proof: ValidationProof,
    ) -> Result<(), Error> {
        self.lce_validation_proofs_db
            .set_lce_validation_proof(subject_id, proof)
    }

    pub fn del_lce_validation_proof(&self, subject_id: &DigestIdentifier) -> Result<(), Error> {
        self.lce_validation_proofs_db
            .del_lce_validation_proof(subject_id)
    }
}
