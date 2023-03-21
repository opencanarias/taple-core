use std::collections::HashMap;

use crate::{commons::bd::db::DB, governance::GovernanceAPI, identifier::DigestIdentifier};

pub struct Notary {
    gov_api: GovernanceAPI,
    database: DB,
    cache_gov_ver: HashMap<DigestIdentifier, u32>,
}

impl Notary {
    pub fn new(gov_api: GovernanceAPI, database: DB) -> Self {
        Self {
            gov_api,
            database,
            cache_gov_ver: HashMap::new(),
        }
    }
}
