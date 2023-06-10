#[derive(Debug, Clone)]
pub enum ValidationStage {
    Approve,
    Evaluate,
    Validate,
    Witness,
    Create,
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
            ValidationStage::Invoke => "invoke",
        }
    }

    pub fn to_role(&self) -> &str {
        match self {
            ValidationStage::Approve => "APPROVER",
            ValidationStage::Evaluate => "EVALUATOR",
            ValidationStage::Validate => "VALIDATOR",
            ValidationStage::Witness => "WITNESS",
            ValidationStage::Create => "CREATOR",
            ValidationStage::Invoke => "INVOKER",
        }
    }
}
