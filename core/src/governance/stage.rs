#[derive(Debug, Clone)]
pub enum ValidationStage {
    Approve,
    Evaluate,
    Validate,
    Witness,
    Create,
    Close,
    Invoke,
}

impl ValidationStage {
    pub fn to_str(&self) -> &str {
        match self {
            ValidationStage::Approve => "approve",
            ValidationStage::Evaluate => "evaluate",
            ValidationStage::Validate => "validate",
            ValidationStage::Witness => "witness",
            ValidationStage::Create => "create",
            ValidationStage::Close => "close",
            ValidationStage::Invoke => "invoke",
        }
    }
}
