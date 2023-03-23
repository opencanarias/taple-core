use crate::database::Error as DbError;
use crate::governance::{GovernanceAPI, GovernanceInterface};
use crate::identifier::Derivable;
use crate::{
    database::DB, evaluator::errors::CompilerErrorResponses, identifier::DigestIdentifier,
    DatabaseManager,
};
use async_std::fs;
use std::path::Path;
use std::process::Command;
use wasm_gc::garbage_collect_file;
use wasmtime::Engine;

use super::NewGovVersion;

pub struct Compiler<D: DatabaseManager, G: GovernanceInterface> {
    database: DB<D>,
    gov_api: G,
    engine: Engine,
    contracts_path: String,
}

impl<D: DatabaseManager, G: GovernanceInterface> Compiler<D, G> {
    pub fn new(database: DB<D>, gov_api: G, engine: Engine, contracts_path: String) -> Self {
        Self {
            database,
            gov_api,
            engine,
            contracts_path,
        }
    }

    pub async fn update_contracts(
        &self,
        compile_info: NewGovVersion,
    ) -> Result<(), CompilerErrorResponses> {
        // TODO: Pillar contrato de base de datos, comprobar si el hash cambia y compilar, si no cambia no compilar
        // Read the contract from database
        let contracts = self
            .gov_api
            .get_contracts(compile_info.governance_id.clone())
            .await
            .map_err(CompilerErrorResponses::GovernanceError)?;
        for contract_info in contracts {
            let contract_data = match self
                .database
                .get_contract(&compile_info.governance_id, &contract_info.0)
            {
                Ok((contract, hash, contract_gov_version)) => {
                    Some((contract, hash, contract_gov_version))
                }
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
            if let Some(contract_data) = contract_data {
                if compile_info.governance_version == contract_data.2 {
                    continue;
                }
                if contract_data.1 == new_contract_hash {
                    self.database
                        .put_contract(
                            &compile_info.governance_id,
                            &contract_info.0,
                            contract_data.0,
                            new_contract_hash,
                            compile_info.governance_version,
                        )
                        .map_err(|error| {
                            CompilerErrorResponses::DatabaseError(error.to_string())
                        })?;
                    continue;
                }
            }
            self.compile(
                contract_info
                    .1
                    .to_string()
                    .map_err(|_| CompilerErrorResponses::AddContractFail)?,
                &compile_info.governance_id.to_str(),
                &contract_info.0,
            )
            .await?;
            let compiled_contract = self
                .add_contract(&compile_info.governance_id.to_str(), &contract_info.0)
                .await?;
            self.database
                .put_contract(
                    &compile_info.governance_id,
                    &contract_info.0,
                    compiled_contract,
                    new_contract_hash,
                    compile_info.governance_version,
                )
                .map_err(|error| CompilerErrorResponses::DatabaseError(error.to_string()))?;
        }
        Ok(())
    }

    async fn compile(
        &self,
        contract: String,
        governance_id: &str,
        schema_id: &str,
    ) -> Result<(), CompilerErrorResponses> {
        let a = format!("{}/src/lib.rs", self.contracts_path);
        let path = Path::new(&a);
        fs::write(format!("{}/src/lib.rs", self.contracts_path), contract)
            .await
            .map_err(|_| CompilerErrorResponses::WriteFileError)?;
        let status = Command::new("cargo")
            .arg("build")
            .arg(format!(
                "--manifest-path={}/Cargo.toml",
                self.contracts_path
            ))
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

        std::fs::create_dir_all(format!(
            "/tmp/taple_contracts/{}/{}",
            governance_id, schema_id
        ))
        .map_err(|_| CompilerErrorResponses::TempFolderCreationFailed)?;

        garbage_collect_file(
            format!(
                "{}/target/wasm32-unknown-unknown/release/contract.wasm",
                self.contracts_path
            ),
            format!(
                "/tmp/taple_contracts/{}/{}/contract.wasm",
                governance_id, schema_id
            ),
        )
        .map_err(|_| CompilerErrorResponses::GarbageCollectorFail)?;
        Ok(())
    }

    async fn add_contract(
        &self,
        governance_id: &str,
        schema_id: &str,
    ) -> Result<Vec<u8>, CompilerErrorResponses> {
        // AOT COMPILATION
        let file = fs::read(format!(
            "/tmp/taple_contracts/{}/{}/contract.wasm",
            governance_id, schema_id
        ))
        .await
        .map_err(|_| CompilerErrorResponses::AddContractFail)?;
        self.engine
            .precompile_module(&file)
            .map_err(|_| CompilerErrorResponses::AddContractFail)
    }
}
