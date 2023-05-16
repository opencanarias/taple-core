use std::collections::HashSet;

use serde::{Serialize, Deserialize};

use crate::{DigestIdentifier, KeyIdentifier};

pub mod error;
pub mod authorized_subjects;
pub mod manager;


#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthorizedSubjectsCommand {
    NewAuthorizedGovernance {
        subject_id: DigestIdentifier,
        providers: HashSet<KeyIdentifier>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthorizedSubjectsResponse {
    NoResponse,
}