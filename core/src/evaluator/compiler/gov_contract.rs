pub fn get_gov_contract() -> String {
    r#"mod sdk;
    use serde::{de::Visitor, Deserialize, Serialize, ser::SerializeMap};
    
    #[derive(Clone)]
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
        roles: Vec<RoleEnum>,
        schema: SchemaEnum,
    }
    
    #[derive(Serialize, Deserialize, Clone)]
    pub enum RoleEnum {
        VALIDATOR,
        CREATOR,
        INVOKER,
        WITNESS,
        APPROVER,
        EVALUATOR
    }
    
    #[derive(Serialize, Deserialize, Clone)]
    pub struct Member {
        id: String,
        name: String,
    }
    
    #[derive(Serialize, Deserialize, Clone)]
    pub struct Contract {
        name: String,
        content: String,
    }
    
    #[derive(Serialize, Deserialize, Clone)]
    pub enum Quorum {
        Majority,
        Fixed(u64), // TODO: Es posible que tenga que ser estructura vacía
        Porcentaje(f64),
        BFT(f64),
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
        state_schema: serde_json::Value, // TODO: QUIZÁS STRING
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
        Patch { data: String },
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
                let patched_state = sdk::apply_patch(&data, &context.initial_state).unwrap();
                *state = patched_state;
                // El usuario debería añadir una función que compruebe el estado del sujeto.
            }
        }
        contract_result.success = true;
        contract_result.approval_required = true;
    }
  "#.into()
}
