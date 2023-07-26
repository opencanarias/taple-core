pub fn get_gov_contract() -> String {
    r#"
use taple_sc_rust as sdk;
use std::collections::HashSet;
use thiserror::Error;
use sdk::ValueWrapper;
use serde::{de::Visitor, ser::SerializeMap, Deserialize, Serialize};

#[derive(Clone)]
#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
pub enum Who {
    ID { ID: String },
    NAME { NAME: String },
    MEMBERS,
    ALL,
    NOT_MEMBERS,
}

impl Serialize for Who {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Who::ID { ID } => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("ID", ID)?;
                map.end()
            }
            Who::NAME { NAME } => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("NAME", NAME)?;
                map.end()
            }
            Who::MEMBERS => serializer.serialize_str("MEMBERS"),
            Who::ALL => serializer.serialize_str("ALL"),
            Who::NOT_MEMBERS => serializer.serialize_str("NOT_MEMBERS"),
        }
    }
}

impl<'de> Deserialize<'de> for Who {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct WhoVisitor;
        impl<'de> Visitor<'de> for WhoVisitor {
            type Value = Who;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("Who")
            }
            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                // Solo deberían tener una entrada
                let Some(key) = map.next_key::<String>()? else {
                    return Err(serde::de::Error::missing_field("ID or NAME"))
                };
                println!("KEY {}", key);
                let result = match key.as_str() {
                    "ID" => {
                        let id: String = map.next_value()?;
                        Who::ID { ID: id }
                    }
                    "NAME" => {
                        let name: String = map.next_value()?;
                        Who::NAME { NAME: name }
                    }
                    _ => return Err(serde::de::Error::unknown_field(&key, &["ID", "NAME"])),
                };
                let None = map.next_key::<String>()? else {
                    return Err(serde::de::Error::custom("Input data is not valid. The data contains unkown entries"));
                };
                Ok(result)
            }
            fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                println!("STR");
                match v.as_str() {
                    "MEMBERS" => Ok(Who::MEMBERS),
                    "ALL" => Ok(Who::ALL),
                    "NOT_MEMBERS" => Ok(Who::NOT_MEMBERS),
                    other => Err(serde::de::Error::unknown_variant(
                        other,
                        &["MEMBERS", "ALL", "NOT_MEMBERS"],
                    )),
                }
            }
            fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                println!("BORR STR");
                match v {
                    "MEMBERS" => Ok(Who::MEMBERS),
                    "ALL" => Ok(Who::ALL),
                    "NOT_MEMBERS" => Ok(Who::NOT_MEMBERS),
                    other => Err(serde::de::Error::unknown_variant(
                        other,
                        &["MEMBERS", "ALL", "NOT_MEMBERS"],
                    )),
                }
            }
        }
        deserializer.deserialize_any(WhoVisitor {})
    }
}

#[derive(Clone)]
#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
pub enum SchemaEnum {
    ID { ID: String },
    NOT_GOVERNANCE,
    ALL,
}

impl Serialize for SchemaEnum {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            SchemaEnum::ID { ID } => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("ID", ID)?;
                map.end()
            }
            SchemaEnum::NOT_GOVERNANCE => serializer.serialize_str("NOT_GOVERNANCE"),
            SchemaEnum::ALL => serializer.serialize_str("ALL"),
        }
    }
}

impl<'de> Deserialize<'de> for SchemaEnum {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct SchemaEnumVisitor;
        impl<'de> Visitor<'de> for SchemaEnumVisitor {
            type Value = SchemaEnum;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("Schema")
            }
            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                // Solo deberían tener una entrada
                let Some(key) = map.next_key::<String>()? else {
                    return Err(serde::de::Error::missing_field("ID"))
                };
                let result = match key.as_str() {
                    "ID" => {
                        let id: String = map.next_value()?;
                        SchemaEnum::ID { ID: id }
                    }
                    _ => return Err(serde::de::Error::unknown_field(&key, &["ID", "NAME"])),
                };
                let None = map.next_key::<String>()? else {
                    return Err(serde::de::Error::custom("Input data is not valid. The data contains unkown entries"));
                };
                Ok(result)
            }
            fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match v.as_str() {
                    "ALL" => Ok(Self::Value::ALL),
                    "NOT_GOVERNANCE" => Ok(Self::Value::NOT_GOVERNANCE),
                    other => Err(serde::de::Error::unknown_variant(
                        other,
                        &["ALL", "NOT_GOVERNANCE"],
                    )),
                }
            }
            fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match v {
                    "ALL" => Ok(Self::Value::ALL),
                    "NOT_GOVERNANCE" => Ok(Self::Value::NOT_GOVERNANCE),
                    other => Err(serde::de::Error::unknown_variant(
                        other,
                        &["ALL", "NOT_GOVERNANCE"],
                    )),
                }
            }
        }
        deserializer.deserialize_any(SchemaEnumVisitor {})
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Role {
    who: Who,
    namespace: String,
    role: RoleEnum,
    schema: SchemaEnum,
}

#[derive(Serialize, Deserialize, Clone)]
pub enum RoleEnum {
    VALIDATOR,
    CREATOR,
    ISSUER,
    WITNESS,
    APPROVER,
    EVALUATOR,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Member {
    id: String,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Contract {
    raw: String,
}

#[derive(Serialize, Deserialize, Clone)]
#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
pub enum Quorum {
    MAJORITY,
    FIXED(u64),
    PERCENTAGE(f64),
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Validation {
    quorum: Quorum,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Policy {
    id: String,
    approve: Validation,
    evaluate: Validation,
    validate: Validation,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Schema {
    id: String,
    schema: serde_json::Value,
    // #[serde(rename = "Initial-Value")]
    initial_value: serde_json::Value,
    contract: Contract,
}

#[repr(C)]
#[derive(Serialize, Deserialize, Clone)]
pub struct Governance {
    members: Vec<Member>,
    roles: Vec<Role>,
    schemas: Vec<Schema>,
    policies: Vec<Policy>,
}

// Definir "Familia de eventos"
#[derive(Serialize, Deserialize, Debug)]
pub enum GovernanceEvent {
    Patch { data: ValueWrapper },
}

#[no_mangle]
pub unsafe fn main_function(state_ptr: i32, event_ptr: i32, is_owner: i32) -> u32 {
    sdk::execute_contract(state_ptr, event_ptr, is_owner, contract_logic)
}

// Lógica del contrato con los tipos de datos esperados
// Devuelve el puntero a los datos escritos con el estado modificado
fn contract_logic(
    context: &sdk::Context<Governance, GovernanceEvent>,
    contract_result: &mut sdk::ContractResult<Governance>,
) {
    // Sería posible añadir gestión de errores
    // Podría ser interesante hacer las operaciones directamente como serde_json:Value en lugar de "Custom Data"
    let state = &mut contract_result.final_state;
    let _is_owner = &context.is_owner;
    match &context.event {
        GovernanceEvent::Patch { data } => {
            // Se recibe un JSON PATCH
            // Se aplica directamente al estado
            let patched_state = sdk::apply_patch(data.0.clone(), &context.initial_state).unwrap();
            if let Ok(_) = check_governance_state(&patched_state) {
                *state = patched_state;
                contract_result.success = true;
                contract_result.approval_required = true;
            } else {
                contract_result.success = false;
            }
        }
    }
}

#[derive(Error, Debug)]
enum StateError {
    #[error("A member's name is duplicated")]
    DuplicatedMemberName,
    #[error("A member's ID is duplicated")]
    DuplicatedMemberID,
    #[error("A policy identifier is duplicated")]
    DuplicatedPolicyID,
    #[error("No governace policy detected")]
    NoGvernancePolicy,
    #[error("It is not allowed to specify a different schema for the governnace")]
    GovernanceShchemaIDDetected,
    #[error("Schema ID is does not have a policy")]
    NoCorrelationSchemaPolicy,
    #[error("There are policies not correlated to any schema")]
    PoliciesWithoutSchema,
    #[error("Role assigned to not defined schema")]
    InvalidRoleSchema,
    #[error("ID specified for Role::Who does not exist")]
    IdWhoRoleNoExist,
    #[error("Name specified for Role::Who does not exist")]
    NameWhoRoleNoExist
}

fn check_governance_state(state: &Governance) -> Result<(), StateError> {
    // Debemos comprobar varios aspectos del estado.
    // No pueden haber miembros duplicados, ya sean en name o en ID
    let (id_set, name_set) = check_members(&state.members)?;
    // No pueden haber policies duplicadas y la asociada a la propia gobernanza debe estar presente
    let policies_names = check_policies(&state.policies)?;
    // No se pueden indicar policies de schema que no existen. Así mismo, no pueden haber
    // schemas sin policies. La correlación debe ser uno-uno
    check_schemas(&state.schemas, policies_names.clone())?;
    check_roles(&state.roles, policies_names, id_set, name_set)
}

fn check_roles(
    roles: &Vec<Role>,
    mut schemas_names: HashSet<String>,
    id_set: HashSet<String>,
    name_set: HashSet<String>,
) -> Result<(), StateError> {
    schemas_names.insert("governance".into());
    for role in roles {
        if let SchemaEnum::ID { ID } = &role.schema {
            if !schemas_names.contains(ID) {
                return Err(StateError::InvalidRoleSchema);
            }
        }
        match &role.who {
            Who::ID { ID } => {
              if !id_set.contains(ID) {
                return Err(StateError::IdWhoRoleNoExist);
              }
            }
            Who::NAME { NAME } => {
              if !name_set.contains(NAME) {
                return Err(StateError::NameWhoRoleNoExist);
              }
            }
            _ => {}
        }
    }
    Ok(())
}

fn check_members(members: &Vec<Member>) -> Result<(HashSet<String>, HashSet<String>), StateError> {
    let mut name_set = HashSet::new();
    let mut id_set = HashSet::new();
    for member in members {
        if name_set.contains(&member.name) {
            return Err(StateError::DuplicatedMemberName);
        }
        name_set.insert(member.name.clone());
        if id_set.contains(&member.id) {
            return Err(StateError::DuplicatedMemberID);
        }
        id_set.insert(member.id.clone());
    }
    Ok((id_set, name_set))
}

fn check_policies(policies: &Vec<Policy>) -> Result<HashSet<String>, StateError> {
    // Se comprueban de que no hayan policies duplicadas y de que se incluya la de gobernanza
    let mut is_governance_present = false;
    let mut id_set = HashSet::new();
    for policy in policies {
        if id_set.contains(&policy.id) {
            return Err(StateError::DuplicatedPolicyID);
        }
        id_set.insert(&policy.id);
        if &policy.id == "governance" {
            is_governance_present = true
        }
    }
    if !is_governance_present {
        return Err(StateError::NoGvernancePolicy);
    }
    id_set.remove(&String::from("governance"));
    Ok(id_set.into_iter().cloned().collect())
}

fn check_schemas(
    schemas: &Vec<Schema>,
    mut policies_names: HashSet<String>,
) -> Result<(), StateError> {
    // Comprobamos que no hayan esquemas duplicados
    // También se tiene que comprobar que los estados iniciales sean válidos según el json_schema
    // Así mismo no puede haber un schema con id "governance"
    for schema in schemas {
        if &schema.id == "governance" {
            return Err(StateError::GovernanceShchemaIDDetected);
        }
        // No pueden haber duplicados y tienen que tener correspondencia con policies_names
        if !policies_names.remove(&schema.id) {
            // No tiene relación con policies_names
            return Err(StateError::NoCorrelationSchemaPolicy);
        }
    }
    if !policies_names.is_empty() {
        return Err(StateError::PoliciesWithoutSchema);
    }
    Ok(())
}
"#.into()
}
