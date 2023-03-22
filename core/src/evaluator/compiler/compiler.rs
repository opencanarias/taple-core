use std::process::Command;

use crate::database::Error as DbError;
use crate::governance::{GovernanceAPI, GovernanceInterface};
use crate::{
    database::DB, evaluator::errors::CompilerErrorResponses, identifier::DigestIdentifier,
    DatabaseManager,
};
use async_std::fs;
use async_std::path::Path;
use json_patch::diff;
use wasm_gc::garbage_collect_file;
use wasmtime::Engine;

use super::CompileContracts;

pub struct Compiler<D: DatabaseManager> {
    database: DB<D>,
    gov_api: GovernanceAPI,
    engine: Engine,
}

impl<D: DatabaseManager> Compiler<D> {
    pub fn new(database: DB<D>, gov_api: GovernanceAPI, engine: Engine) -> Self {
        Self {
            database,
            gov_api,
            engine,
        }
    }
    
    pub async fn compile(&self, compile_info: CompileContracts) -> Result<(), CompilerErrorResponses> {
        // TODO: Pillar contrato de base de datos, comprobar si el hash cambia y compilar, si no cambia no compilar
        // Read the contract from database
        let contracts = self
            .gov_api
            .get_contracts(compile_info.governance_id.clone())
            .await
            .map_err(CompilerErrorResponses::GovernanceError)?;
        for contract_info in contracts {
            let contract_hash = match self
                .database
                .get_contract(&compile_info.governance_id, &contract_info.0)
            {
                Ok((contract, hash)) => Some(hash),
                Err(DbError::EntryNotFound) => {
                    // Añadir en la response
                    None
                }
                Err(error) => return Err(CompilerErrorResponses::DatabaseError(error.to_string())),
            };
            let new_contract_hash = contract_info
                .1
                .get_digest()
                .map_err(|_| CompilerErrorResponses::BorshSerializeContractError)?;
            if let Some(contract_hash) = contract_hash {
                if contract_hash == new_contract_hash {
                    continue;
                }
            }
            compile(contract_info.1.to_string()).await;
            let compiled_contract = self.add_contract(String::from("hola")).await?;
            self.database
                .put_contract(
                    &compile_info.governance_id,
                    &contract_info.0,
                    compiled_contract,
                    new_contract_hash,
                )
                .map_err(|error| CompilerErrorResponses::DatabaseError(error.to_string()))?;
        }
        Ok(())
    }

    async fn add_contract(
        &self,
        file: impl AsRef<Path>,
    ) -> Result<Vec<u8>, CompilerErrorResponses> {
        // AOT COMPILATION
        let file = fs::read(&file)
            .await
            .map_err(|_| CompilerErrorResponses::AddContractFail)?;
        self.engine
            .precompile_module(&file)
            .map_err(|_| CompilerErrorResponses::AddContractFail)
    }
}

async fn compile(contract: String) -> Result<(), CompilerErrorResponses> {
    fs::write("./smart_contracts/src/lib.rs", contract)
        .await
        .map_err(|_| CompilerErrorResponses::WriteFileError)?;
    let status = Command::new("cargo")
        .arg("build")
        .arg("--manifest-path=./smart_contracts/Cargo.toml")
        .arg("--target")
        .arg("wasm32-unknown-unknown")
        .arg("--release")
        .output()
        // No muestra stdout. Genera proceso hijo y espera
        .map_err(|_| CompilerErrorResponses::CargoExecError)?;
    if !status.status.success() {
        return Err(CompilerErrorResponses::CargoExecError);
    }
    // Utilidad para optimizar el Wasm resultante
    // Es una API, así que requiere de Wasm-gc en el sistema
    garbage_collect_file(
        "./smart_contracts/target/wasm32-unknown-unknown/release/contract.wasm",
        "./compiled_contracts/contrac.wasm",
    )
    .map_err(|_| CompilerErrorResponses::GarbageCollectorFail)?;
    Ok(())
}
