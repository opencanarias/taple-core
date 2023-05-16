use crate::{database::DB, DatabaseManager};


pub struct AuthorizedSubjects<D: DatabaseManager> {
    database: DB<D>,
}

impl<D: DatabaseManager> AuthorizedSubjects<D> {
    pub fn new(database: DB<D>) -> Self {
        Self { database }
    }
}
