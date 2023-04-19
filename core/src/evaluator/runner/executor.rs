use crate::{
    evaluator::{errors::ExecutorErrorResponses, Context},
    governance::GovernanceInterface,
};

use super::context::MemoryManager;
use serde::{Deserialize, Serialize};
use wasmtime::{Caller, Engine, Linker, Module, Store};

#[derive(Serialize, Deserialize)]
pub struct ContractResult {
    pub final_state: String,
    pub approval_required: bool,
    pub success: bool,
}

pub struct ContractExecutor<G: GovernanceInterface + Send> {
    engine: Engine,
    gov_api: G,
}

impl<G: GovernanceInterface + Send> ContractExecutor<G> {
    pub fn new(engine: Engine, gov_api: G) -> Self {
        Self { engine, gov_api }
    }

    pub async fn execute_contract(
        &self,
        state: &str,
        event: &str,
        context: &Context,
        governance_version: u64,
        compiled_contract: Vec<u8>,
    ) -> Result<ContractResult, ExecutorErrorResponses> {
        // Obtener Roles del usuario
        let roles = self
            .gov_api
            .get_roles_of_invokator(
                &context.invokator,
                &context.governance_id,
                governance_version,
                &context.schema_id,
                &context.namespace,
            )
            .await
            .map_err(|_| ExecutorErrorResponses::RolesObtentionFailed)?;
        // Cargar wasm
        let module = unsafe { Module::deserialize(&self.engine, compiled_contract).unwrap() };
        // Generar contexto
        let (context, state_ptr, event_ptr, roles_ptr) = self.generate_context(state, event, roles);
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
            .call(&mut store, (state_ptr, event_ptr, roles_ptr))
            .map_err(|_| ExecutorErrorResponses::ContractExecutionFailed)?;
        // Obtención "NEW STATE" almacenado en el contexto
        Ok(self.get_result(&store, result_ptr)?)
    }

    fn generate_context(
        &self,
        state: &str,
        event: &str,
        roles: Vec<String>,
    ) -> (MemoryManager, u32, u32, u32) {
        let mut context = MemoryManager::new();
        let state_ptr = context.add_date_raw(state.as_bytes());
        let event_ptr = context.add_date_raw(event.as_bytes());
        let roles_ptr = context.add_data(roles);
        (
            context,
            state_ptr as u32,
            event_ptr as u32,
            roles_ptr as u32,
        )
    }

    fn get_result(
        &self,
        store: &Store<MemoryManager>,
        pointer: u32,
    ) -> Result<ContractResult, ExecutorErrorResponses> {
        let bytes = store.data().read_data(pointer as usize)?;
        Ok(bincode::deserialize(bytes)
            .map_err(|_| ExecutorErrorResponses::CantGenerateContractResult)?)
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
        Ok(linker)
    }
}
