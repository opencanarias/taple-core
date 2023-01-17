use commons::schema_handler::{get_governance_schema, Schema};

pub fn get_schema_json_schema() -> Schema {
    let schema = get_governance_schema();
    let compiled = Schema::compile(&schema);
    match compiled {
        Ok(schema) => schema,
        Err(e) => {
            println!("{:?}", e);
            panic!("a");
        }
    }
}

#[cfg(test)]
mod tests {
    use commons::schema_handler::Schema;
    use serde_json::json;
    
    #[test]
    fn test_meta_schema() {
        let schema = json!({"a": {"$ref": "http://json-schema.org/draft/2020-12/schema"}});
        let compiled = Schema::compile(&schema);
        let json_schema = match compiled {
            Ok(json_schema) => json_schema,
            Err(e) => {
                println!("{:?}", e);
                panic!("a");
            }
        };
        let instance = json!({ "a": {
            "type": "object",
            "additionalProperties": false,
            "required": [
                "members",
                "schemas"
                ],
            "properties": {
                "members": {
                    "type": "array",
                    "minItems": 1, // There must be a minimum of one member
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": {"type": "string"},
                            "tags": {
                                "type": "object",
                                "patternProperties": {
                                    "^.*$": {
                                    "anyOf": [
                                        {"type": "string"},
                                        {"type": "null"}
                                    ]
                                    }
                                },
                                "additionalProperties": false
                            },
                            "description": {"type": "string"},
                            "key": {"type": "string"},
                        },
                        "required": ["id", "tags", "key"],
                        "additionalProperties": false
                    }
                },
                "schemas": {
                    "type": "array",
                    "minItems": 0,
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": {"type": "string"},
                            "tags": {
                                "type": "object",
                                "patternProperties": {
                                    "^.*$": {
                                    "anyOf": [
                                        {"type": "string"},
                                        {"type": "null"}
                                    ]
                                    }
                                },
                                "additionalProperties": false
                            },
                            "content": {
                                "$ref": "http://json-schema.org/draft/2020-12/schema",
                            },
                        },
                        "required": [
                            "id",
                            "tags",
                            "content"
                        ],
                        "additionalProperties": false,
                    },
                },
            }
        }});
        assert!(json_schema.validate(&instance));
    }
}
