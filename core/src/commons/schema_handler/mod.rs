use jsonschema::JSONSchema;
use serde_json::{json, Value};
use std::str::FromStr;

pub mod gov_models;

use crate::commons::{errors::Error, identifier::KeyIdentifier};

#[derive(Debug)]
pub struct Schema {
    json_schema: JSONSchema,
}

impl Schema {
    pub fn compile(schema: &Value) -> Result<Self, Error> {
        match JSONSchema::options()
            .with_format("keyidentifier", validate_gov_keyidentifiers)
            .compile(&schema)
        {
            Ok(json_schema) => Ok(Schema { json_schema }),
            Err(_) => Err(Error::SchemaCreationError),
        }
    }

    pub fn validate(&self, value: &Value) -> bool {
        match self.json_schema.validate(value) {
            Ok(_) => true,
            Err(_) => false,
        }
    }
}

fn validate_gov_keyidentifiers(key: &str) -> bool {
    match KeyIdentifier::from_str(key) {
        Ok(_) => true,
        Err(_) => false,
    }
}

pub fn get_governance_schema() -> Value {
    json!({
      "$defs": {
        "roles": {
          "type": "array",
          "items": {
            "type": "string"
          }
        },
        "quorum": {
          "oneOf": [
            {
              "const": "majority"
            },
            {
              "type": "object",
              "properties": {
                "fixed": {
                  "type": "number",
                  "minimum": 1,
                  "multipleOf": 1
                }
              },
              "required": ["Fixed"],
              "additionalProperties": false
            },
            {
              "type": "object",
              "properties": {
                "porcentaje": {
                  "type": "number",
                  "minimum": 0,
                  "maximum": 1
                }
              },
              "required": ["Porcentaje"],
              "additionalProperties": false
            },
            {
              "type": "object",
              "properties": {
                "BFT": {
                  "type": "number",
                  "minimum": 0,
                  "maximum": 1
                }
              },
              "required": ["BFT"],
              "additionalProperties": false
            }
          ]
        },
        "validation": {
          "type": "object",
                "additionalProperties": false,
                "required": ["roles", "quorum"],
                "roles": {
                  "$ref": "#/$defs/roles"
                },
                "quorum": {
                  "$ref": "#/$defs/quorum"
                }
        }
      },
      "type": "object",
      "additionalProperties": false,
      "required": [
        "members",
        "schemas",
        "policies",
        "roles"
      ],
      "properties": {
        "members": {
          "type": "array",
          "items": {
            "type": "object",
            "properties": {
              "id": {
                "type": "string"
              },
              "description": {
                "type": "string"
              },
              "key": {
                "type": "string",
                "format": "keyidentifier"
              }
            },
            "required": [
              "id",
              "key"
            ],
            "additionalProperties": false
          }
        },
        "roles": {
          "type": "array",
          "items": {
            "type": "object",
            "properties": {
              "who": {
                "oneOf": [
                {
                  "type": "object",
                  "properties": {
                    "id": {
                      "type": "string"
                    }
                  },
                  "required": ["id"],
                  "additionalProperties": false
                },
                {
                  "const": "members"
                },
                {
                  "const": "all"
                },
                {
                  "const": "external"
                }
              ]
            },
            "namespace": {
              "type": "string"
            },
            "roles": {
              "$ref": "#/$defs/roles"
            },
            "schema": {
              "oneOf": [
                {
                  "type": "object",
                  "properties": {
                    "id": {
                      "type": "string"
                    }
                  },
                  "required": ["id"],
                  "additionalProperties": false
                },
                {
                  "const": "all_schemas"
                }
                ]
              }
            },
            "required": ["who", "roles", "schema", "namespace"],
            "additionalProperties": false
          }
        },
        "schemas": {
          "type": "array",
          "minItems": 0,
          "items": {
            "type": "object",
            "properties": {
              "id": {
                "type": "string"
              },
              "state_schema": {
                "$schema": "http://json-schema.org/draft/2020-12/schema",
                "$id": "http://json-schema.org/draft/2020-12/schema",
                "$vocabulary": {
                  "http://json-schema.org/draft/2020-12/vocab/core": true,
                  "http://json-schema.org/draft/2020-12/vocab/applicator": true,
                  "http://json-schema.org/draft/2020-12/vocab/unevaluated": true,
                  "http://json-schema.org/draft/2020-12/vocab/validation": true,
                  "http://json-schema.org/draft/2020-12/vocab/meta-data": true,
                  "http://json-schema.org/draft/2020-12/vocab/format-annotation": true,
                  "http://json-schema.org/draft/2020-12/vocab/content": true
                },
                "$dynamicAnchor": "meta",
                "title": "Core and validation specifications meta-schema",
                "allOf": [
                  {
                    "$schema": "https://json-schema.org/draft/2020-12/schema",
                    "$id": "https://json-schema.org/draft/2020-12/meta/core",
                    "$vocabulary": {
                      "https://json-schema.org/draft/2020-12/vocab/core": true
                    },
                    "$dynamicAnchor": "meta",
                    "title": "Core vocabulary meta-schema",
                    "type": [
                      "object",
                      "boolean"
                    ],
                    "properties": {
                      "$id": {
                        "$ref": "#/$defs/uriReferenceString",
                        "$comment": "Non-empty fragments not allowed.",
                        "pattern": "^[^#]*#?$"
                      },
                      "$schema": {
                        "$ref": "#/$defs/uriString"
                      },
                      "$ref": {
                        "$ref": "#/$defs/uriReferenceString"
                      },
                      "$anchor": {
                        "$ref": "#/$defs/anchorString"
                      },
                      "$dynamicRef": {
                        "$ref": "#/$defs/uriReferenceString"
                      },
                      "$dynamicAnchor": {
                        "$ref": "#/$defs/anchorString"
                      },
                      "$vocabulary": {
                        "type": "object",
                        "propertynames": {
                          "$ref": "#/$defs/uriString"
                        },
                        "additionalProperties": {
                          "type": "boolean"
                        }
                      },
                      "$comment": {
                        "type": "string"
                      },
                      "$defs": {
                        "type": "object",
                        "additionalProperties": {
                          "$dynamicRef": "#meta"
                        }
                      }
                    },
                    "$defs": {
                      "anchorString": {
                        "type": "string",
                        "pattern": "^[A-Za-z_][-A-Za-z0-9._]*$"
                      },
                      "uriString": {
                        "type": "string",
                        "format": "uri"
                      },
                      "uriReferenceString": {
                        "type": "string",
                        "format": "uri-reference"
                      }
                    }
                  },
                  {
                    "$schema": "https://json-schema.org/draft/2020-12/schema",
                    "$id": "https://json-schema.org/draft/2020-12/meta/applicator",
                    "$vocabulary": {
                      "https://json-schema.org/draft/2020-12/vocab/applicator": true
                    },
                    "$dynamicAnchor": "meta",
                    "title": "Applicator vocabulary meta-schema",
                    "type": [
                      "object",
                      "boolean"
                    ],
                    "properties": {
                      "prefixItems": {
                        "$ref": "#/$defs/schemaArray"
                      },
                      "items": {
                        "$dynamicRef": "#meta"
                      },
                      "contains": {
                        "$dynamicRef": "#meta"
                      },
                      "additionalProperties": {
                        "$dynamicRef": "#meta"
                      },
                      "properties": {
                        "type": "object",
                        "additionalProperties": {
                          "$dynamicRef": "#meta"
                        },
                        "default": {}
                      },
                      "patternProperties": {
                        "type": "object",
                        "additionalProperties": {
                          "$dynamicRef": "#meta"
                        },
                        "propertynames": {
                          "format": "regex"
                        },
                        "default": {}
                      },
                      "dependentschemas": {
                        "type": "object",
                        "additionalProperties": {
                          "$dynamicRef": "#meta"
                        },
                        "default": {}
                      },
                      "propertynames": {
                        "$dynamicRef": "#meta"
                      },
                      "if": {
                        "$dynamicRef": "#meta"
                      },
                      "then": {
                        "$dynamicRef": "#meta"
                      },
                      "else": {
                        "$dynamicRef": "#meta"
                      },
                      "allOf": {
                        "$ref": "#/$defs/schemaArray"
                      },
                      "anyOf": {
                        "$ref": "#/$defs/schemaArray"
                      },
                      "oneOf": {
                        "$ref": "#/$defs/schemaArray"
                      },
                      "not": {
                        "$dynamicRef": "#meta"
                      }
                    },
                    "$defs": {
                      "schemaArray": {
                        "type": "array",
                        "minItems": 1,
                        "items": {
                          "$dynamicRef": "#meta"
                        }
                      }
                    }
                  },
                  {
                    "$schema": "https://json-schema.org/draft/2020-12/schema",
                    "$id": "https://json-schema.org/draft/2020-12/meta/unevaluated",
                    "$vocabulary": {
                      "https://json-schema.org/draft/2020-12/vocab/unevaluated": true
                    },
                    "$dynamicAnchor": "meta",
                    "title": "Unevaluated applicator vocabulary meta-schema",
                    "type": [
                      "object",
                      "boolean"
                    ],
                    "properties": {
                      "unevaluatedItems": {
                        "$dynamicRef": "#meta"
                      },
                      "unevaluatedProperties": {
                        "$dynamicRef": "#meta"
                      }
                    }
                  },
                  {
                    "$schema": "https://json-schema.org/draft/2020-12/schema",
                    "$id": "https://json-schema.org/draft/2020-12/meta/validation",
                    "$vocabulary": {
                      "https://json-schema.org/draft/2020-12/vocab/validation": true
                    },
                    "$dynamicAnchor": "meta",
                    "title": "validation vocabulary meta-schema",
                    "type": [
                      "object",
                      "boolean"
                    ],
                    "properties": {
                      "type": {
                        "anyOf": [
                          {
                            "$ref": "#/$defs/simpleTypes"
                          },
                          {
                            "type": "array",
                            "items": {
                              "$ref": "#/$defs/simpleTypes"
                            },
                            "minItems": 1,
                            "uniqueItems": true
                          }
                        ]
                      },
                      "const": true,
                      "enum": {
                        "type": "array",
                        "items": true
                      },
                      "multipleOf": {
                        "type": "number",
                        "exclusiveMinimum": 0
                      },
                      "maximum": {
                        "type": "number"
                      },
                      "exclusiveMaximum": {
                        "type": "number"
                      },
                      "minimum": {
                        "type": "number"
                      },
                      "exclusiveMinimum": {
                        "type": "number"
                      },
                      "maxLength": {
                        "$ref": "#/$defs/nonNegativeInteger"
                      },
                      "minLength": {
                        "$ref": "#/$defs/nonNegativeIntegerDefault0"
                      },
                      "pattern": {
                        "type": "string",
                        "format": "regex"
                      },
                      "maxItems": {
                        "$ref": "#/$defs/nonNegativeInteger"
                      },
                      "minItems": {
                        "$ref": "#/$defs/nonNegativeIntegerDefault0"
                      },
                      "uniqueItems": {
                        "type": "boolean",
                        "default": false
                      },
                      "maxContains": {
                        "$ref": "#/$defs/nonNegativeInteger"
                      },
                      "minContains": {
                        "$ref": "#/$defs/nonNegativeInteger",
                        "default": 1
                      },
                      "maxProperties": {
                        "$ref": "#/$defs/nonNegativeInteger"
                      },
                      "minProperties": {
                        "$ref": "#/$defs/nonNegativeIntegerDefault0"
                      },
                      "required": {
                        "$ref": "#/$defs/stringArray"
                      },
                      "dependentRequired": {
                        "type": "object",
                        "additionalProperties": {
                          "$ref": "#/$defs/stringArray"
                        }
                      }
                    },
                    "$defs": {
                      "nonNegativeInteger": {
                        "type": "integer",
                        "minimum": 0
                      },
                      "nonNegativeIntegerDefault0": {
                        "$ref": "#/$defs/nonNegativeInteger",
                        "default": 0
                      },
                      "simpleTypes": {
                        "enum": [
                          "array",
                          "boolean",
                          "integer",
                          "null",
                          "number",
                          "object",
                          "string"
                        ]
                      },
                      "stringArray": {
                        "type": "array",
                        "items": {
                          "type": "string"
                        },
                        "uniqueItems": true,
                        "default": []
                      }
                    }
                  },
                  {
                    "$schema": "https://json-schema.org/draft/2020-12/schema",
                    "$id": "https://json-schema.org/draft/2020-12/meta/meta-data",
                    "$vocabulary": {
                      "https://json-schema.org/draft/2020-12/vocab/meta-data": true
                    },
                    "$dynamicAnchor": "meta",
                    "title": "Meta-data vocabulary meta-schema",
                    "type": [
                      "object",
                      "boolean"
                    ],
                    "properties": {
                      "title": {
                        "type": "string"
                      },
                      "description": {
                        "type": "string"
                      },
                      "default": true,
                      "deprecated": {
                        "type": "boolean",
                        "default": false
                      },
                      "readOnly": {
                        "type": "boolean",
                        "default": false
                      },
                      "writeOnly": {
                        "type": "boolean",
                        "default": false
                      },
                      "examples": {
                        "type": "array",
                        "items": true
                      }
                    }
                  },
                  {
                    "$schema": "https://json-schema.org/draft/2020-12/schema",
                    "$id": "https://json-schema.org/draft/2020-12/meta/format-annotation",
                    "$vocabulary": {
                      "https://json-schema.org/draft/2020-12/vocab/format-annotation": true
                    },
                    "$dynamicAnchor": "meta",
                    "title": "Format vocabulary meta-schema for annotation results",
                    "type": [
                      "object",
                      "boolean"
                    ],
                    "properties": {
                      "format": {
                        "type": "string"
                      }
                    }
                  },
                  {
                    "$schema": "https://json-schema.org/draft/2020-12/schema",
                    "$id": "https://json-schema.org/draft/2020-12/meta/content",
                    "$vocabulary": {
                      "https://json-schema.org/draft/2020-12/vocab/content": true
                    },
                    "$dynamicAnchor": "meta",
                    "title": "content vocabulary meta-schema",
                    "type": [
                      "object",
                      "boolean"
                    ],
                    "properties": {
                      "contentEncoding": {
                        "type": "string"
                      },
                      "contentMediaType": {
                        "type": "string"
                      },
                      "contentschema": {
                        "$dynamicRef": "#meta"
                      }
                    }
                  }
                ],
                "type": [
                  "object",
                  "boolean"
                ],
                "$comment": "This meta-schema also defines keywords that have appeared in previous drafts in order to prevent incompatible extensions as they remain in common use.",
                "properties": {
                  "definitions": {
                    "$comment": "\"definitions\" has been replaced by \"$defs\".",
                    "type": "object",
                    "additionalProperties": {
                      "$dynamicRef": "#meta"
                    },
                    "deprecated": true,
                    "default": {}
                  },
                  "dependencies": {
                    "$comment": "\"dependencies\" has been split and replaced by \"dependentschemas\" and \"dependentRequired\" in order to serve their differing semantics.",
                    "type": "object",
                    "additionalProperties": {
                      "anyOf": [
                        {
                          "$dynamicRef": "#meta"
                        },
                        {
                          "$ref": "meta/validation#/$defs/stringArray"
                        }
                      ]
                    },
                    "deprecated": true,
                    "default": {}
                  },
                  "$recursiveAnchor": {
                    "$comment": "\"$recursiveAnchor\" has been replaced by \"$dynamicAnchor\".",
                    "$ref": "meta/core#/$defs/anchorString",
                    "deprecated": true
                  },
                  "$recursiveRef": {
                    "$comment": "\"$recursiveRef\" has been replaced by \"$dynamicRef\".",
                    "$ref": "meta/core#/$defs/uriReferenceString",
                    "deprecated": true
                  }
                }
              },
              "initial_value": {},
              "contract": {
                "type": "object",
                "properties": {
                  "name": {
                    "type": "string"
                  },
                  "content": {
                    "type": "string"
                  },
                },
                "additionalProperties": false,
                "required": ["name", "content"]
              },
              "facts": {
                "type": "object",
                "properties": {
                  "name": {
                    "type": "string"
                  },
                  "description": {
                    "type": "string"
                  },
                  "schema":{}
                },
                "additionalProperties": false,
                "required": ["name", "schema"]
              }
            },
            "required": [
              "id",
              "state_schema",
              "initial_value",
              "contract",
              "facts"
            ],
            "additionalProperties": false
          }
        },
        "policies": {
          "type": "array",
          "items": {
            "type": "object",
            "additionalProperties": false,
            "required": [
              "id", "approve", "evaluate", "validate", "create", "witness", "close", "invoke"
            ],
            "properties": {
              "id": {
                "type": "string"
              },
              "approve": {
                "$ref": "#/$defs/validation"
              },
              "evaluate": {
                "$ref": "#/$defs/validation"
              },
              "validate": {
                "$ref": "#/$defs/validation"
              },
              "create": {
                "$ref": "#/$defs/roles"
              },
              "witness": {
                "$ref": "#/$defs/roles"
              },
              "close": {
                "$ref": "#/$defs/roles"
              },
              "invoke": {
                "type": "array",
                "items": {
                  "type": "object",
                  "properties": {
                    "fact": {
                      "type": "string"
                    },
                    "approval_required": {
                      "type": "boolean"
                    },
                    "roles": {
                      "$ref": "#/$defs/roles"
                    }
                  },
                  "additionalProperties": false,
                  "required": [
                    "fact", "approval_required", "roles"
                  ]
                }
              }
            }
          }
        }
      }
    })
}
