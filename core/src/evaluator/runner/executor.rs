use crate::{evaluator::errors::ExecutorErrorResponses, ValueWrapper};

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

pub struct ContractExecutor {
    engine: Engine,
}

impl ContractExecutor {
    pub fn new(engine: Engine) -> Self {
        Self { engine }
    }

    pub async fn execute_contract(
        &self,
        state: &ValueWrapper,
        event: &ValueWrapper,
        compiled_contract: Vec<u8>,
        is_owner: bool,
    ) -> Result<ContractResult, ExecutorErrorResponses> {
        // Cargar wasm
        let module = unsafe { Module::deserialize(&self.engine, compiled_contract).unwrap() };
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
