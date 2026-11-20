use crate::host_lifecycle::HostManager;
use chatos_local_agent_protocol::{HostCommand, HostResult};
use std::{future::Future, pin::Pin};

pub(crate) type HostCommandFuture<'a> =
    Pin<Box<dyn Future<Output = Result<HostResult, String>> + Send + 'a>>;

pub(crate) trait HostCommandClient: Send + Sync {
    fn execute(&self, command: HostCommand) -> HostCommandFuture<'_>;
}

impl HostCommandClient for HostManager {
    fn execute(&self, command: HostCommand) -> HostCommandFuture<'_> {
        Box::pin(HostManager::execute(self, command))
    }
}
