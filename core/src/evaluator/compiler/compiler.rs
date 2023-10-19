use crate::database::Error as DbError;
use crate::evaluator::errors::CompilerError;
use crate::governance::GovernanceInterface;
use crate::identifier::{Derivable, DigestIdentifier};
use crate::{database::DB, evaluator::errors::CompilerErrorResponses, DatabaseCollection};
use async_std::fs;
use log::{debug, info};
use std::collections::HashSet;
use std::path::Path;
use std::process::Command;
use wasmtime::{Engine, ExternType};


pub struct Compiler<C: DatabaseCollection, G: GovernanceInterface> {
    database: DB<C>,
    gov_api: G,
    engine: Engine,
    contracts_path: String,
    available_imports_set: HashSet<String>,
}

impl<C: DatabaseCollection, G: GovernanceInterface> Compiler<C, G> {
    pub fn new(database: DB<C>, gov_api: G, engine: Engine, contracts_path: String) -> Self {
        let available_imports_set = get_sdk_functions_identifier();
        Self {
            database,
            gov_api,
            engine,
            contracts_path,
            available_imports_set,
        }
    }

    pub async fn init(&self) -> Result<(), CompilerError> {
        // Checks if the governance contract exists in the system
        // If it does not exist, it compiles and saves it.
        if !Path::new(&self.contracts_path).exists() {
            return Err(CompilerError::InternalError(CompilerErrorResponses::FolderNotFound));
        }
        Ok(())
    }

    pub async fn update_contracts(
        &self,
        governance_id: DigestIdentifier,
        governance_version: u64,
    ) -> Result<(), CompilerErrorResponses> {
        // TODO: Pick contract from database, check if hash changes and compile, if it doesn't change don't compile
        // Read the contract from database
        let contracts = self
            .gov_api
            .get_contracts(governance_id.clone(), governance_version)
            .await
            .map_err(CompilerErrorResponses::GovernanceError)?;
        for (contract_info, schema_id) in contracts {
            let contract_data = match self.database.get_contract(&governance_id, &schema_id) {
                Ok((contract, hash, contract_gov_version)) => {
                    Some((contract, hash, contract_gov_version))
                }
                Err(DbError::EntryNotFound) => {
                    // Add in the response
                    None
                }
                Err(error) => return Err(CompilerErrorResponses::DatabaseError(error.to_string())),
            };
            let new_contract_hash =
                DigestIdentifier::generate_with_blake3(&contract_info.raw)
                    .map_err(|_| CompilerErrorResponses::BorshSerializeContractError)?;
            if let Some(contract_data) = contract_data {
                if governance_version == contract_data.2 {
                    continue;
                }
                if contract_data.1 == new_contract_hash {
                    // The associated governance version is updated.
                    self.database
                        .put_contract(
                            &governance_id,
                            &schema_id,
                            contract_data.0,
                            new_contract_hash,
                            governance_version,
                        )
                        .map_err(|error| {
                            CompilerErrorResponses::DatabaseError(error.to_string())
                        })?;
                    continue;
                }
            }
            self.compile(
                contract_info.raw,
                &governance_id.to_str(),
                &schema_id,
                governance_version,
            )
            .await?;
            let compiled_contract = self.add_contract().await?;
            self.database
                .put_contract(
                    &governance_id,
                    &schema_id,
                    compiled_contract,
                    new_contract_hash,
                    governance_version,
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
        sn: u64,
    ) -> Result<(), CompilerErrorResponses> {
        fs::write(format!("{}/src/lib.rs", self.contracts_path), contract)
            .await
            .map_err(|_| CompilerErrorResponses::WriteFileError)?;
        info!("Compiling contract: {} {} {}", schema_id, governance_id, sn);
        let status = Command::new("cargo")
            .arg("build")
            .arg("--offline")
            .arg(format!(
                "--manifest-path={}/Cargo.toml",
                self.contracts_path
            ))
            .arg("--target")
            .arg("wasm32-unknown-unknown")
            .arg("--release")
            .output()
            // Does not show stdout. Generates child process and waits
            .map_err(|_| CompilerErrorResponses::CargoExecError)?;
        info!(
            "Compiled success contract: {} {} {}",
            schema_id, governance_id, sn
        );
        debug!("status {:?}", status);
        if !status.status.success() {
            return Err(CompilerErrorResponses::CargoExecError);
        }

        std::fs::create_dir_all(format!(
            "/tmp/taple_contracts/{}/{}",
            governance_id, schema_id
        ))
        .map_err(|_| CompilerErrorResponses::TempFolderCreationFailed)?;

        Ok(())
    }

    async fn add_contract(&self) -> Result<Vec<u8>, CompilerErrorResponses> {
        // AOT COMPILATION
        let file = fs::read(format!(
            "{}/target/wasm32-unknown-unknown/release/contract.wasm",
            self.contracts_path
        ))
        .await
        .map_err(|_| CompilerErrorResponses::AddContractFail)?;
        let module_bytes = self
            .engine
            .precompile_module(&file)
            .map_err(|_| CompilerErrorResponses::AddContractFail)?;
        let module = unsafe { wasmtime::Module::deserialize(&self.engine, &module_bytes).unwrap() };
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
                _ => return Err(CompilerErrorResponses::InvalidImportFound),
            }
        }
        if !pending_sdk.is_empty() {
            return Err(CompilerErrorResponses::NoSDKFound);
        }
        Ok(module_bytes)
    }
}

fn get_sdk_functions_identifier() -> HashSet<String> {
    HashSet::from_iter(
        vec![
            "alloc".to_owned(),
            "write_byte".to_owned(),
            "pointer_len".to_owned(),
            "read_byte".to_owned(),
            // "cout".to_owned(),
        ]
        .into_iter(),
    )
}
