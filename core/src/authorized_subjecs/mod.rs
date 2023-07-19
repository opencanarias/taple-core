use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::{DigestIdentifier, KeyIdentifier};

pub mod authorized_subjects;
pub mod error;
pub mod manager;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthorizedSubjectsCommand {
    NewAuthorizedSubject {
        subject_id: DigestIdentifier,
        providers: HashSet<KeyIdentifier>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthorizedSubjectsResponse {
    NoResponse,
}
