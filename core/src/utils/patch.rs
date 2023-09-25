use json_patch::{Patch, patch};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq)]
pub enum PatchErrors {
    #[error("JSON provided is not of patch type")]
    JsonIsNotPatch,
    #[error("Error generating the Patch: {0}")]
    PatchGenerationError(String),
    #[error("Error expressing patch as JSON")]
    PatchToJsonFailed
}

pub fn apply_patch<State: for<'a> Deserialize<'a> + Serialize>(
  patch_arg: Value,
  mut state: Value,
) -> Result<State, PatchErrors> {
  let patch_data: Patch = serde_json::from_value(patch_arg).map_err(|_| PatchErrors::JsonIsNotPatch)?;
  patch(&mut state, &patch_data).map_err(|e| PatchErrors::PatchGenerationError(e.to_string()))?;
  Ok(serde_json::from_value(state).map_err(|_| PatchErrors::PatchToJsonFailed)?)
}