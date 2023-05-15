pub fn get_governance_initial_state() -> serde_json::Value {
    serde_json::json!({
        "members": [],
        "roles": [
          {
            "who": "Members",
            "namespace": "",
            "roles": [
              "Members"
            ],
            "schema": "governance"
          }
        ],
        "schemas": [],
        "policies": [
          {
            "id": "governance",
            "approve": {
              "roles": [
                "Members",
                "Owner"
              ],
              "quorum": "Majority"
            },
            "evaluate": {
              "roles": [
                "Members",
                "Owner"
              ],
              "quorum": "Majority"
            },
            "validate": {
              "roles": [
                "Members",
                "Owner"
              ],
              "quorum": "Majority"
            },
            "create": [
              "Members",
              "Owner"
            ],
            "witness": [
              "Members",
              "Owner"
            ],
            "close": [
              "Members",
              "Owner"
            ],
            "invoke": [
              "Members",
              "Owner"
            ]
          }
        ]
    })
}
