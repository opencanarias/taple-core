use std::{sync::Arc, time::Duration};

use commons::models::{signature::Signature, state::SubjectData};
use core::{ApiModuleInterface, NodeAPI};
use futures::{future, FutureExt};

#[allow(dead_code)]
pub async fn do_task_with_timeout<Output>(
    future: future::BoxFuture<'static, Output>,
    ms: u64,
) -> Result<Output, tokio::time::error::Elapsed> {
    tokio::time::timeout(Duration::from_millis(ms), future).await
}

#[allow(dead_code)]
pub async fn get_subject_with_timeout(
    taple: Arc<NodeAPI>,
    id: String,
    ms: u64,
) -> Result<SubjectData, tokio::time::error::Elapsed> {
    do_task_with_timeout(
        async move {
            loop {
                let subject = taple.get_subject(id.clone()).await;
                if subject.is_ok() {
                    return subject.unwrap();
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
        .boxed(),
        ms,
    )
    .await
}

#[allow(dead_code)]
pub async fn get_signatures_with_timeout(
    taple: Arc<NodeAPI>,
    subject_id: String,
    sn: u64,
    expected_signatures: usize,
    ms: u64,
) -> Result<Vec<Signature>, tokio::time::error::Elapsed> {
    do_task_with_timeout(
        async move {
            loop {
                let signatures = taple
                    .get_signatures(subject_id.clone(), sn, None, None)
                    .await;
                if signatures.is_ok() {
                    let tmp = signatures.unwrap();
                    if tmp.len() == expected_signatures {
                        return tmp;
                    }
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
        .boxed(),
        ms,
    )
    .await
}

#[allow(dead_code)]
pub fn governance_one() -> serde_json::Value {
    serde_json::json!({
            "members": [
                {
                    "id": "Open Canarias",
                    "tags": {},
                    "description": "a",
                    "key": "EFXv0jBIr6BtoqFMR7G_JBSuozRc2jZnu5VGUH2gy6-w"
                },
            ],
            "schemas": [
                {
                    "id": "prueba",
                    "tags": {},
                    "content": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": [
                            "a"
                        ],
                        "properties": {
                            "a": {"type": "string"}
                        }
                    }
                }
            ],
            "policies": [
                {
                    "id": "prueba",
                    "validation": {
                        "quorum": 0.5,
                        "validators": [
                            "EFXv0jBIr6BtoqFMR7G_JBSuozRc2jZnu5VGUH2gy6-w"
                        ]
                    },
                    "approval": {
                        "quorum": 0.5,
                        "approvers": [
                            "EFXv0jBIr6BtoqFMR7G_JBSuozRc2jZnu5VGUH2gy6-w"
                        ]
                    },
                    "invokation": {
                        "owner": {
                            "allowance": true,
                            "approvalRequired": true
                        },
                        "set": {
                            "allowance": false,
                            "approvalRequired": false,
                            "invokers": []
                        },
                        "all": {
                            "allowance": false,
                            "approvalRequired": false,
                        },
                        "external": {
                            "allowance": false,
                            "approvalRequired": false
                        }
                    }
                },
                {
                    "id": "governance",
                    "validation": {
                        "quorum": 0.5,
                        "validators": [
                            "EFXv0jBIr6BtoqFMR7G_JBSuozRc2jZnu5VGUH2gy6-w"
                        ]
                    },
                    "approval": {
                        "quorum": 0.5,
                        "approvers": [
                            "EFXv0jBIr6BtoqFMR7G_JBSuozRc2jZnu5VGUH2gy6-w"
                        ]
                    },
                    "invokation": {
                        "owner": {
                            "allowance": true,
                            "approvalRequired": true
                        },
                        "set": {
                            "allowance": false,
                            "approvalRequired": false,
                            "invokers": []
                        },
                        "all": {
                            "allowance": true,
                            "approvalRequired": true,
                        },
                        "external": {
                            "allowance": false,
                            "approvalRequired": false
                        }
                    }
                },
            ]
        }
    )
}

#[allow(dead_code)]
pub fn governance_two() -> serde_json::Value {
    serde_json::json!({
            "members": [
                {
                    "id": "Open Canarias",
                    "tags": {},
                    "description": "a",
                    "key": "EFXv0jBIr6BtoqFMR7G_JBSuozRc2jZnu5VGUH2gy6-w"
                },
                {
                    "id": "Acciona",
                    "tags": {},
                    "description": "b",
                    "key": "ECQnl-h1vEWmu-ZlPuweR3N1x6SUImyVdPrCLmnJJMyU",
                },
            ],
            "schemas": [
                {
                    "id": "prueba",
                    "tags": {},
                    "content": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": [
                            "a"
                        ],
                        "properties": {
                            "a": {"type": "string"}
                        }
                    }
                }
            ],
            "policies": [
                {
                    "id": "prueba",
                    "validation": {
                        "quorum": 0.5,
                        "validators": [
                            "EFXv0jBIr6BtoqFMR7G_JBSuozRc2jZnu5VGUH2gy6-w",
                            "ECQnl-h1vEWmu-ZlPuweR3N1x6SUImyVdPrCLmnJJMyU"
                        ]
                    },
                    "approval": {
                        "quorum": 0.5,
                        "approvers": [
                            "EFXv0jBIr6BtoqFMR7G_JBSuozRc2jZnu5VGUH2gy6-w",
                            "ECQnl-h1vEWmu-ZlPuweR3N1x6SUImyVdPrCLmnJJMyU"
                        ]
                    },
                    "invokation": {
                        "owner": {
                            "allowance": true,
                            "approvalRequired": true
                        },
                        "set": {
                            "allowance": false,
                            "approvalRequired": false,
                            "invokers": []
                        },
                        "all": {
                            "allowance": true,
                            "approvalRequired": true,
                        },
                        "external": {
                            "allowance": false,
                            "approvalRequired": false
                        }
                    }
                },
                {
                    "id": "governance",
                    "validation": {
                        "quorum": 0.5,
                        "validators": [
                            "EFXv0jBIr6BtoqFMR7G_JBSuozRc2jZnu5VGUH2gy6-w",
                            "ECQnl-h1vEWmu-ZlPuweR3N1x6SUImyVdPrCLmnJJMyU"
                        ]
                    },
                    "approval": {
                        "quorum": 0.5,
                        "approvers": [
                            "EFXv0jBIr6BtoqFMR7G_JBSuozRc2jZnu5VGUH2gy6-w",
                            "ECQnl-h1vEWmu-ZlPuweR3N1x6SUImyVdPrCLmnJJMyU"
                        ]
                    },
                    "invokation": {
                        "owner": {
                            "allowance": true,
                            "approvalRequired": true
                        },
                        "set": {
                            "allowance": false,
                            "approvalRequired": false,
                            "invokers": []
                        },
                        "all": {
                            "allowance": true,
                            "approvalRequired": true,
                        },
                        "external": {
                            "allowance": false,
                            "approvalRequired": false
                        }
                    }
                },
            ]
    })
}

#[allow(dead_code)]
pub fn governance_three() -> serde_json::Value {
    serde_json::json!({
            "members": [
                {
                    "id": "Open Canarias",
                    "tags": {},
                    "description": "a",
                    "key": "EFXv0jBIr6BtoqFMR7G_JBSuozRc2jZnu5VGUH2gy6-w"
                },
                {
                    "id": "Acciona",
                    "tags": {},
                    "description": "b",
                    "key": "ECQnl-h1vEWmu-ZlPuweR3N1x6SUImyVdPrCLmnJJMyU",
                },
                {
                    "id": "Iberdrola",
                    "tags": {},
                    "description": "c",
                    "key": "EejcG-XG-dR991FEGR2Y3PefeKa5v0yTOXl80azRwgOk"
                }
            ],
            "schemas": [
                {
                    "id": "prueba",
                    "tags": {},
                    "content": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": [
                            "a"
                        ],
                        "properties": {
                            "a": {"type": "string"}
                        }
                    }
                }
            ],
            "policies": [
                {
                    "id": "prueba",
                    "validation": {
                        "quorum": 0.5,
                        "validators": [
                            "EFXv0jBIr6BtoqFMR7G_JBSuozRc2jZnu5VGUH2gy6-w",
                            "ECQnl-h1vEWmu-ZlPuweR3N1x6SUImyVdPrCLmnJJMyU",
                            "EejcG-XG-dR991FEGR2Y3PefeKa5v0yTOXl80azRwgOk"
                        ]
                    },
                    "approval": {
                        "quorum": 0.5,
                        "approvers": [
                            "EFXv0jBIr6BtoqFMR7G_JBSuozRc2jZnu5VGUH2gy6-w",
                            "ECQnl-h1vEWmu-ZlPuweR3N1x6SUImyVdPrCLmnJJMyU",
                            "EejcG-XG-dR991FEGR2Y3PefeKa5v0yTOXl80azRwgOk"
                        ]
                    },
                    "invokation": {
                        "owner": {
                            "allowance": true,
                            "approvalRequired": true
                        },
                        "set": {
                            "allowance": false,
                            "approvalRequired": false,
                            "invokers": []
                        },
                        "all": {
                            "allowance": true,
                            "approvalRequired": true,
                        },
                        "external": {
                            "allowance": true,
                            "approvalRequired": true
                        }
                    }
                },
                {
                    "id": "governance",
                    "validation": {
                        "quorum": 0.5,
                        "validators": [
                            "EFXv0jBIr6BtoqFMR7G_JBSuozRc2jZnu5VGUH2gy6-w",
                            "ECQnl-h1vEWmu-ZlPuweR3N1x6SUImyVdPrCLmnJJMyU",
                            "EejcG-XG-dR991FEGR2Y3PefeKa5v0yTOXl80azRwgOk"
                        ]
                    },
                    "approval": {
                        "quorum": 0.5,
                        "approvers": [
                            "EFXv0jBIr6BtoqFMR7G_JBSuozRc2jZnu5VGUH2gy6-w",
                            "ECQnl-h1vEWmu-ZlPuweR3N1x6SUImyVdPrCLmnJJMyU",
                            "EejcG-XG-dR991FEGR2Y3PefeKa5v0yTOXl80azRwgOk"
                        ]
                    },
                    "invokation": {
                        "owner": {
                            "allowance": true,
                            "approvalRequired": true
                        },
                        "set": {
                            "allowance": false,
                            "approvalRequired": false,
                            "invokers": []
                        },
                        "all": {
                            "allowance": true,
                            "approvalRequired": true,
                        },
                        "external": {
                            "allowance": false,
                            "approvalRequired": false
                        }
                    }
                },
            ]
        }
    )
}

#[allow(dead_code)]
pub fn governance_incorrect_schema_policy() -> serde_json::Value {
    serde_json::json!({
            "members": [
                {
                    "id": "Open Canarias",
                    "tags": {},
                    "description": "a",
                    "key": "EFXv0jBIr6BtoqFMR7G_JBSuozRc2jZnu5VGUH2gy6-w"
                },
            ],
            "schemas": [
                {
                    "id": "prueba",
                    "tags": {},
                    "content": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": [
                            "a"
                        ],
                        "properties": {
                            "a": {"type": "string"}
                        }
                    }
                }
            ],
            "policies": [
                {
                    "id": "incorrect",
                    "validation": {
                        "quorum": 0.5,
                        "validators": [
                            "EFXv0jBIr6BtoqFMR7G_JBSuozRc2jZnu5VGUH2gy6-w",
                        ]
                    },
                    "approval": {
                        "quorum": 0.5,
                        "approvers": [
                            "EFXv0jBIr6BtoqFMR7G_JBSuozRc2jZnu5VGUH2gy6-w",
                        ]
                    },
                    "invokation": {
                        "owner": {
                            "allowance": true,
                            "approvalRequired": true
                        },
                        "set": {
                            "allowance": false,
                            "approvalRequired": false,
                            "invokers": []
                        },
                        "all": {
                            "allowance": false,
                            "approvalRequired": false,
                        },
                        "external": {
                            "allowance": true,
                            "approvalRequired": true
                        }
                    }
                },
                {
                    "id": "governance",
                    "validation": {
                        "quorum": 0.5,
                        "validators": [
                            "EFXv0jBIr6BtoqFMR7G_JBSuozRc2jZnu5VGUH2gy6-w"
                        ]
                    },
                    "approval": {
                        "quorum": 0.5,
                        "approvers": [
                            "EFXv0jBIr6BtoqFMR7G_JBSuozRc2jZnu5VGUH2gy6-w"
                        ]
                    },
                    "invokation": {
                        "owner": {
                            "allowance": true,
                            "approvalRequired": true
                        },
                        "set": {
                            "allowance": false,
                            "approvalRequired": false,
                            "invokers": []
                        },
                        "all": {
                            "allowance": true,
                            "approvalRequired": true,
                        },
                        "external": {
                            "allowance": false,
                            "approvalRequired": false
                        }
                    }
                },
            ]
        }
    )
}

#[allow(dead_code)]
pub fn governance_incorrect_member_in_policy() -> serde_json::Value {
    serde_json::json!({
            "members": [
                {
                    "id": "Open Canarias",
                    "tags": {},
                    "description": "a",
                    "key": "EFXv0jBIr6BtoqFMR7G_JBSuozRc2jZnu5VGUH2gy6-w"
                },
            ],
            "schemas": [
                {
                    "id": "prueba",
                    "tags": {},
                    "content": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": [
                            "a"
                        ],
                        "properties": {
                            "a": {"type": "string"}
                        }
                    }
                }
            ],
            "policies": [
                {
                    "id": "prueba",
                    "validation": {
                        "quorum": 0.5,
                        "validators": [
                            "ECQnl-h1vEWmu-ZlPuweR3N1x6SUImyVdPrCLmnJJMyU"
                        ]
                    },
                    "approval": {
                        "quorum": 0.5,
                        "approvers": [
                            "EFXv0jBIr6BtoqFMR7G_JBSuozRc2jZnu5VGUH2gy6-w"
                        ]
                    },
                    "invokation": {
                        "owner": {
                            "allowance": true,
                            "approvalRequired": true
                        },
                        "set": {
                            "allowance": false,
                            "approvalRequired": false,
                            "invokers": []
                        },
                        "all": {
                            "allowance": false,
                            "approvalRequired": false,
                        },
                        "external": {
                            "allowance": true,
                            "approvalRequired": true
                        }
                    }
                },
                {
                    "id": "governance",
                    "validation": {
                        "quorum": 0.5,
                        "validators": [
                            "EFXv0jBIr6BtoqFMR7G_JBSuozRc2jZnu5VGUH2gy6-w"
                        ]
                    },
                    "approval": {
                        "quorum": 0.5,
                        "approvers": [
                            "EFXv0jBIr6BtoqFMR7G_JBSuozRc2jZnu5VGUH2gy6-w"
                        ]
                    },
                    "invokation": {
                        "owner": {
                            "allowance": true,
                            "approvalRequired": true
                        },
                        "set": {
                            "allowance": false,
                            "approvalRequired": false,
                            "invokers": []
                        },
                        "all": {
                            "allowance": true,
                            "approvalRequired": true,
                        },
                        "external": {
                            "allowance": false,
                            "approvalRequired": false
                        }
                    }
                },
            ]
        }
    )
}
