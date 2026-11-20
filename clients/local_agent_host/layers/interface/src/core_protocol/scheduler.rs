#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalAgentResourcePressureLevel {
    Normal,
    Elevated,
    Serious,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SetSchedulerPressureCommand {
    pub pressure_level: LocalAgentResourcePressureLevel,
    pub target_concurrent_work_items: usize,
}

impl SetSchedulerPressureCommand {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=16).contains(&self.target_concurrent_work_items) {
            return Err("target_concurrent_work_items must be between 1 and 16".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SetSchedulerAdmissionCommand {
    pub accepting_new_work: bool,
}
