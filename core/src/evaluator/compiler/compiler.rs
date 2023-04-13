use crate::database::Error as DbError;
use crate::governance::{GovernanceInterface};
use crate::identifier::Derivable;
use crate::{
    database::DB, evaluator::errors::CompilerErrorResponses,
    DatabaseManager,
};
use async_std::fs;
use std::collections::HashSet;
use std::path::Path;
use std::process::Command;
use wasm_gc::garbage_collect_file;
use wasmtime::{Engine, ExternType};

use super::NewGovVersion;

pub struct Compiler<D: DatabaseManager, G: GovernanceInterface> {
    database: DB<D>,
    gov_api: G,
    engine: Engine,
    contracts_path: String,
    available_imports_set: HashSet<String>
}

impl<D: DatabaseManager, G: GovernanceInterface> Compiler<D, G> {
    pub fn new(database: DB<D>, gov_api: G, engine: Engine, contracts_path: String) -> Self {
        let available_imports_set = get_sdk_functions_identifier();
        Self {
            database,
            gov_api,
            engine,
            contracts_path,
            available_imports_set
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
        let module_bytes =self.engine
            .precompile_module(&file)
            .map_err(|_| CompilerErrorResponses::AddContractFail)?;
        let module = unsafe {
            wasmtime::Module::deserialize(
                &self.engine,
                &module_bytes,
            )
            .unwrap()
        };
        let imports = module.imports();
        let mut pending_sdk = self.available_imports_set.clone();
        for import in imports {
            match import.ty() {
                ExternType::Func(_) => {
                    if !self.available_imports_set.contains(import.name()) {
                        return Err(CompilerErrorResponses::InvalidImportFound);
                    }
                    pending_sdk.remove(import.name());
                }
                _ => return Err(CompilerErrorResponses::InvalidImportFound)
            }
        }
        println!("{:?}", pending_sdk);
        if !pending_sdk.is_empty() {
            return Err(CompilerErrorResponses::NoSDKFound);
        }
        Ok(module_bytes)
    }
}

fn get_sdk_functions_identifier() -> HashSet<String> {
    HashSet::from_iter(vec![
        "alloc".to_owned(),
        "write_byte".to_owned(),
        "pointer_len".to_owned(),
        "read_byte".to_owned()
    ].into_iter())
}
