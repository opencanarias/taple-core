#[derive(Debug)]
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
            ValidationStage::Approve => "Approve",
            ValidationStage::Evaluate => "Evaluate",
            ValidationStage::Validate => "Validate",
            ValidationStage::Witness => "Witness",
            ValidationStage::Create => "Create",
            ValidationStage::Close => "Close",
            ValidationStage::Invoke => "Invoke",
        }
    }
}
