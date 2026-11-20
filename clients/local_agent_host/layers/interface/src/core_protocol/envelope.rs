#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HostRequestEnvelope {
    pub protocol_version: u32,
    pub command_id: String,
    pub command: HostCommand,
}

impl HostRequestEnvelope {
    pub fn validate(&self) -> Result<(), String> {
        if self.protocol_version != LOCAL_AGENT_PROTOCOL_VERSION {
            return Err(format!(
                "unsupported protocol version {}; expected {}",
                self.protocol_version, LOCAL_AGENT_PROTOCOL_VERSION
            ));
        }
        validate_identifier("command_id", &self.command_id)?;
        self.command.validate()
    }
}
