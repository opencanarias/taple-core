pub fn get_gov_contract() -> String {
    r#"mod sdk;
  use serde::{Deserialize, Serialize};
  
  #[derive(Clone)]
  pub enum Who {
      Who(String), // TODO: QUIZÁS DEBERÍA SER UNA STRUCT ANÓNIMA CON STRING, YA QUE EN EL SCHEMA SE PONE COMO OBJECT
      Members,
      All,
      External,
  }
  
  impl Serialize for Who {
      fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
      where
          S: serde::Serializer,
      {
          match self {
              Who::Who(s) => serializer.serialize_str(&s),
              Who::Members => serializer.serialize_str("Members"),
              Who::All => serializer.serialize_str("All"),
              Who::External => serializer.serialize_str("External"),
          }
      }
  }
  
  impl<'de> Deserialize<'de> for Who {
      fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
      where
          D: serde::Deserializer<'de>,
      {
          struct SchemaEnumVisitor;
          impl<'de> serde::de::Visitor<'de> for SchemaEnumVisitor {
              fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                  formatter.write_str("Who")
              }
              type Value = Who;
              fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
              where
                  E: serde::de::Error,
              {
                  match v.as_str() {
                      "Members" => Ok(Who::Members),
                      "All" => Ok(Who::All),
                      "External" => Ok(Who::External),
                      &_ => Ok(Self::Value::Who(v)),
                  }
              }
              fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Self::Value, E>
              where
                  E: serde::de::Error,
              {
                  match v {
                      "Members" => Ok(Who::Members),
                      "All" => Ok(Who::All),
                      "External" => Ok(Who::External),
                      &_ => Ok(Self::Value::Who(v.to_string())),
                  }
              }
          }
          deserializer.deserialize_str(SchemaEnumVisitor {})
      }
  }
  
  #[derive(Clone)]
  pub enum SchemaEnum {
      Schema(String), // TODO: QUIZÁS DEBERÍA SER UNA STRUCT ANÓNIMA CON STRING, YA QUE EN EL SCHEMA SE PONE COMO OBJECT
      AllSchemas,
  }
  
  impl Serialize for SchemaEnum {
      fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
      where
          S: serde::Serializer,
      {
          match self {
              SchemaEnum::Schema(s) => serializer.serialize_str(&s),
              SchemaEnum::AllSchemas => serializer.serialize_str("All_Schemas"),
          }
      }
  }
  
  impl<'de> Deserialize<'de> for SchemaEnum {
      fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
      where
          D: serde::Deserializer<'de>,
      {
          struct SchemaEnumVisitor;
          impl<'de> serde::de::Visitor<'de> for SchemaEnumVisitor {
              fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                  formatter.write_str("SchemaEnum")
              }
              type Value = SchemaEnum;
              fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
              where
                  E: serde::de::Error,
              {
                  match v.as_str() {
                      "All_Schemas" => Ok(Self::Value::AllSchemas),
                      &_ => Ok(Self::Value::Schema(v)),
                  }
              }
              fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Self::Value, E>
              where
                  E: serde::de::Error,
              {
                  match v {
                      "All_Schemas" => Ok(Self::Value::AllSchemas),
                      &_ => Ok(Self::Value::Schema(v.to_string())),
                  }
              }
          }
          deserializer.deserialize_str(SchemaEnumVisitor {})
      }
  }
  
  #[derive(Serialize, Deserialize, Clone)]
  pub struct Role {
      who: Who,
      namespace: String,
      roles: Vec<String>,
      schema: SchemaEnum,
  }
  
  #[derive(Serialize, Deserialize, Clone)]
  pub struct Member {
      id: String,
      description: String,
      key: String,
  }
  
  #[derive(Serialize, Deserialize, Clone)]
  pub struct Contract {
      name: String,
      content: String,
  }
  
  #[derive(Serialize, Deserialize, Clone)]
  pub struct Fact {
      name: String,
      description: String,
      schema: serde_json::Value,
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
      roles: Vec<String>,
      quorum: Quorum,
  }
  
  #[derive(Serialize, Deserialize, Clone)]
  pub struct Policy {
      id: String,
      approve: Validation,
      evaluate: Validation,
      validate: Validation,
      create: Vec<String>,
      witness: Vec<String>,
      close: Vec<String>,
      invoke: Vec<String>,
  }
  
  #[derive(Serialize, Deserialize, Clone)]
  pub struct Schema {
      id: String,
      state_schema: serde_json::Value, // TODO: QUIZÁS STRING
      // #[serde(rename = "Initial-Value")]
      // Initial_Value:
      contract: Contract,
      facts: Vec<Fact>,
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
  pub unsafe fn main_function(state_ptr: i32, event_ptr: i32, roles_ptr: i32) -> u32 {
      sdk::execute_contract(state_ptr, event_ptr, roles_ptr, contract_logic)
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
      let _roles = &context.roles;
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
