pub fn get_governance_initial_state() -> serde_json::Value {
    serde_json::json!({
        "members": [],
        "roles": [],
        "schemas": [],
        "policies": [
          {
            "id": "governance",
            "approve": {
              "quorum": "MAJORITY"
            },
            "evaluate": {
              "quorum": "MAJORITY"
            },
            "validate": {
              "quorum": "MAJORITY"
            }
          }
        ]
    })
}

#[cfg(test)]
mod test {
    use crate::commons::schema_handler::{get_governance_schema, Schema};

    #[test]
    fn compile_initial_state() {
        let gov_schema = get_governance_schema();
        let schema = Schema::compile(&gov_schema).expect("gov schema compiles");
        let init_state = super::get_governance_initial_state();
        assert!(schema.validate(&init_state))
        // let schema =
    }
}
