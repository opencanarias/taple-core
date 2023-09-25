use std::collections::HashSet;

use crate::{
    commons::schema_handler::gov_models::{
        Governance, GovernanceEvent, Member, Policy, Role, Schema, SchemaEnum, Who,
    },
    evaluator::errors::{ExecutorErrorResponses, GovernanceStateError},
    utils::patch::apply_patch,
    ValueWrapper,
};

use super::context::MemoryManager;
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use wasmtime::{Caller, Engine, Linker, Module, Store};

#[derive(Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
struct WasmContractResult {
    pub final_state: ValueWrapper,
    pub approval_required: bool,
    pub success: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ContractResult {
    pub final_state: ValueWrapper,
    pub approval_required: bool,
    pub success: bool,
}

impl ContractResult {
    pub fn error() -> Self {
        Self {
            final_state: ValueWrapper(serde_json::Value::Null),
            approval_required: false,
            success: false,
        }
    }
}

pub enum Contract {
    CompiledContract(Vec<u8>),
    GovContract,
}

pub struct ContractExecutor {
    engine: Engine,
}

impl ContractExecutor {
    pub fn new(engine: Engine) -> Self {
        Self { engine }
    }

    async fn execute_gov_contract(
        &self,
        state: &ValueWrapper,
        event: &ValueWrapper,
    ) -> Result<ContractResult, ExecutorErrorResponses> {
        let Ok(event) = serde_json::from_value::<GovernanceEvent>(event.0.clone()) else {
            return Ok(ContractResult::error())
        };
        match &event {
            GovernanceEvent::Patch { data } => {
                let Ok(patched_state) = apply_patch(data.0.clone(), state.0.clone()) else {
                    return Ok(ContractResult::error());
                };
                if let Ok(_) = check_governance_state(&patched_state) {
                    Ok(ContractResult {
                        final_state: ValueWrapper(serde_json::to_value(patched_state).unwrap()),
                        approval_required: true,
                        success: true,
                    })
                } else {
                    Ok(ContractResult {
                        final_state: state.clone(),
                        approval_required: false,
                        success: false,
                    })
                }
            }
        }
    }

    pub async fn execute_contract(
        &self,
        state: &ValueWrapper,
        event: &ValueWrapper,
        compiled_contract: Contract,
        is_owner: bool,
    ) -> Result<ContractResult, ExecutorErrorResponses> {
        let Contract::CompiledContract(contract_bytes) = compiled_contract else {
            return self.execute_gov_contract(
                state, event
            ).await;
        };
        // Cargar wasm
        let module = unsafe { Module::deserialize(&self.engine, contract_bytes).unwrap() };
        // Generar contexto
        let (context, state_ptr, event_ptr) = self.generate_context(&state, &event)?;
        let mut store = Store::new(&self.engine, context);
        // Generar Linker
        let linker = self.generate_linker(&self.engine)?;
        // Generar instancia contrato
        let instance = linker
            .instantiate(&mut store, &module)
            .map_err(|_| ExecutorErrorResponses::ContractNotInstantiated)?;
        // Ejecución contrato
        let contract_entrypoint = instance
            .get_typed_func::<(u32, u32, u32), u32>(&mut store, "main_function")
            .map_err(|_| ExecutorErrorResponses::ContractEntryPointNotFound)?;
        let result_ptr = contract_entrypoint
            .call(
                &mut store,
                (state_ptr, event_ptr, if is_owner { 1 } else { 0 }),
            )
            .map_err(|_| ExecutorErrorResponses::ContractExecutionFailed)?;
        // Obtención "NEW STATE" almacenado en el contexto
        let contract_result = self.get_result(&store, result_ptr)?;
        Ok(contract_result)
    }

    fn generate_context(
        &self,
        state: &ValueWrapper,
        event: &ValueWrapper,
    ) -> Result<(MemoryManager, u32, u32), ExecutorErrorResponses> {
        let mut context = MemoryManager::new();
        let state_ptr = context.add_data_raw(
            &state
                .try_to_vec()
                .map_err(|_| ExecutorErrorResponses::BorshSerializationError)?,
        );
        let event_ptr = context.add_data_raw(
            &event
                .try_to_vec()
                .map_err(|_| ExecutorErrorResponses::BorshSerializationError)?,
        );
        Ok((context, state_ptr as u32, event_ptr as u32))
    }

    fn get_result(
        &self,
        store: &Store<MemoryManager>,
        pointer: u32,
    ) -> Result<ContractResult, ExecutorErrorResponses> {
        let bytes = store.data().read_data(pointer as usize)?;
        let contract_result: WasmContractResult = BorshDeserialize::try_from_slice(bytes)
            .map_err(|_| ExecutorErrorResponses::CantGenerateContractResult)?;
        let result = ContractResult {
            final_state: contract_result.final_state,
            approval_required: contract_result.approval_required,
            success: contract_result.success,
        };
        Ok(result)
    }

    fn generate_linker(
        &self,
        engine: &Engine,
    ) -> Result<Linker<MemoryManager>, ExecutorErrorResponses> {
        let mut linker = Linker::new(&engine);
        linker
            .func_wrap(
                "env",
                "pointer_len",
                |caller: Caller<'_, MemoryManager>, pointer: i32| {
                    return caller.data().get_pointer_len(pointer as usize) as u32;
                },
            )
            .map_err(|_| ExecutorErrorResponses::FunctionLinkingFailed("pointer_len".to_owned()))?;
        linker
            .func_wrap(
                "env",
                "alloc",
                |mut caller: Caller<'_, MemoryManager>, len: u32| {
                    return caller.data_mut().alloc(len as usize) as u32;
                },
            )
            .map_err(|_| ExecutorErrorResponses::FunctionLinkingFailed("alloc".to_owned()))?;
        linker
            .func_wrap(
                "env",
                "write_byte",
                |mut caller: Caller<'_, MemoryManager>, ptr: u32, offset: u32, data: u32| {
                    return caller
                        .data_mut()
                        .write_byte(ptr as usize, offset as usize, data as u8);
                },
            )
            .map_err(|_| ExecutorErrorResponses::FunctionLinkingFailed("write_byte".to_owned()))?;
        linker
            .func_wrap(
                "env",
                "read_byte",
                |caller: Caller<'_, MemoryManager>, index: i32| {
                    return caller.data().read_byte(index as usize) as u32;
                },
            )
            .map_err(|_| ExecutorErrorResponses::FunctionLinkingFailed("read_byte".to_owned()))?;
        linker
            .func_wrap(
                "env",
                "cout",
                |_caller: Caller<'_, MemoryManager>, ptr: u32| {
                    println!("{}", ptr);
                },
            )
            .expect("Failed write_byte link");
        Ok(linker)
    }
}

fn check_governance_state(state: &Governance) -> Result<(), GovernanceStateError> {
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
) -> Result<(), GovernanceStateError> {
    schemas_names.insert("governance".into());
    for role in roles {
        if let SchemaEnum::ID { ID } = &role.schema {
            if !schemas_names.contains(ID) {
                return Err(GovernanceStateError::InvalidRoleSchema);
            }
        }
        match &role.who {
            Who::ID { ID } => {
                if !id_set.contains(ID) {
                    return Err(GovernanceStateError::IdWhoRoleNoExist);
                }
            }
            Who::NAME { NAME } => {
                if !name_set.contains(NAME) {
                    return Err(GovernanceStateError::NameWhoRoleNoExist);
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn check_members(
    members: &Vec<Member>,
) -> Result<(HashSet<String>, HashSet<String>), GovernanceStateError> {
    let mut name_set = HashSet::new();
    let mut id_set = HashSet::new();
    for member in members {
        if name_set.contains(&member.name) {
            return Err(GovernanceStateError::DuplicatedMemberName);
        }
        name_set.insert(member.name.clone());
        if id_set.contains(&member.id) {
            return Err(GovernanceStateError::DuplicatedMemberID);
        }
        id_set.insert(member.id.clone());
    }
    Ok((id_set, name_set))
}

fn check_policies(policies: &Vec<Policy>) -> Result<HashSet<String>, GovernanceStateError> {
    // Se comprueban de que no hayan policies duplicadas y de que se incluya la de gobernanza
    let mut is_governance_present = false;
    let mut id_set = HashSet::new();
    for policy in policies {
        if id_set.contains(&policy.id) {
            return Err(GovernanceStateError::DuplicatedPolicyID);
        }
        id_set.insert(&policy.id);
        if &policy.id == "governance" {
            is_governance_present = true
        }
    }
    if !is_governance_present {
        return Err(GovernanceStateError::NoGvernancePolicy);
    }
    id_set.remove(&String::from("governance"));
    Ok(id_set.into_iter().cloned().collect())
}

fn check_schemas(
    schemas: &Vec<Schema>,
    mut policies_names: HashSet<String>,
) -> Result<(), GovernanceStateError> {
    // Comprobamos que no hayan esquemas duplicados
    // También se tiene que comprobar que los estados iniciales sean válidos según el json_schema
    // Así mismo no puede haber un schema con id "governance"
    for schema in schemas {
        if &schema.id == "governance" {
            return Err(GovernanceStateError::GovernanceShchemaIDDetected);
        }
        // No pueden haber duplicados y tienen que tener correspondencia con policies_names
        if !policies_names.remove(&schema.id) {
            // No tiene relación con policies_names
            return Err(GovernanceStateError::NoCorrelationSchemaPolicy);
        }
    }
    if !policies_names.is_empty() {
        return Err(GovernanceStateError::PoliciesWithoutSchema);
    }
    Ok(())
}
