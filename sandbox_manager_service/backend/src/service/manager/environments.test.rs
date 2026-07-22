// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::models::SandboxEnvironmentMcpPolicy;

    use super::*;

    fn application(service_id: &str) -> PreparedEnvironmentService {
        PreparedEnvironmentService {
            input: SandboxEnvironmentServiceInput {
                service_id: service_id.to_string(),
                environment_key: service_id.to_string(),
                display_name: service_id.to_string(),
                service_role: "application".to_string(),
                image_id: Some("default".to_string()),
                image_ref: None,
                dockerfile: Some("FROM alpine\n".to_string()),
                environment: BTreeMap::new(),
                mcp_policy: SandboxEnvironmentMcpPolicy {
                    managed_by: "system".to_string(),
                    attachment: "project_gateway_target".to_string(),
                    filesystem: true,
                    terminal: true,
                },
            },
            image_ref: "chatos-sandbox-agent:latest".to_string(),
        }
    }

    #[test]
    fn multiple_applications_require_program_selected_primary_service() {
        let services = vec![application("api"), application("worker")];
        assert!(resolve_primary_service_id(None, services.as_slice()).is_err());
        assert_eq!(
            resolve_primary_service_id(Some("worker"), services.as_slice())
                .expect("selected primary"),
            "worker"
        );
    }

    #[test]
    fn dependencies_cannot_receive_mcp_policy() {
        let dependency = SandboxEnvironmentServiceInput {
            service_id: "redis".to_string(),
            environment_key: "redis".to_string(),
            display_name: "Redis".to_string(),
            service_role: "dependency".to_string(),
            image_id: None,
            image_ref: Some("redis:7-alpine".to_string()),
            dockerfile: None,
            environment: BTreeMap::new(),
            mcp_policy: SandboxEnvironmentMcpPolicy {
                managed_by: "system".to_string(),
                attachment: "project_gateway_target".to_string(),
                filesystem: true,
                terminal: true,
            },
        };
        assert!(validate_dependency_service(&dependency).is_err());
    }

    #[test]
    fn application_dockerfile_is_forwarded_but_dependency_never_enables_mcp() {
        let application = application("api");
        let application_spec = backend_environment_service_spec(&application);
        assert_eq!(
            application_spec.dockerfile.as_deref(),
            Some("FROM alpine\n")
        );
        assert!(application_spec.mcp_enabled);

        let dependency = PreparedEnvironmentService {
            input: SandboxEnvironmentServiceInput {
                service_id: "redis".to_string(),
                environment_key: "redis".to_string(),
                display_name: "Redis".to_string(),
                service_role: "dependency".to_string(),
                image_id: None,
                image_ref: Some("redis:7-alpine".to_string()),
                dockerfile: None,
                environment: BTreeMap::new(),
                mcp_policy: SandboxEnvironmentMcpPolicy::default(),
            },
            image_ref: "redis:7-alpine".to_string(),
        };
        let dependency_spec = backend_environment_service_spec(&dependency);
        assert!(!dependency_spec.mcp_enabled);
        assert!(dependency_spec.dockerfile.is_none());
    }

    #[test]
    fn callers_cannot_inject_mcp_environment_or_agent_installation() {
        let mut environment = BTreeMap::new();
        environment.insert(
            "CHATOS_SANDBOX_MCP_TOKEN".to_string(),
            "caller-token".to_string(),
        );
        assert!(validate_environment_values(&environment).is_err());

        let mut application = application("api").input;
        application.dockerfile =
            Some("FROM alpine\nCOPY chatos-sandbox-mcp-server /usr/local/bin/\n".to_string());
        assert!(validate_application_service(&application).is_err());
    }

    #[test]
    fn dependency_targets_are_forbidden_for_terminal_and_mcp_routes() {
        let dependency = SandboxEnvironmentServiceRecord {
            service_id: "redis".to_string(),
            environment_key: "redis".to_string(),
            display_name: "Redis".to_string(),
            service_role: "dependency".to_string(),
            image_id: None,
            image_ref: "redis:7-alpine".to_string(),
            backend_id: Some("container-1".to_string()),
            status: "running".to_string(),
            agent_endpoint: None,
            mcp_policy: SandboxEnvironmentMcpPolicy::default(),
        };
        assert!(ensure_terminal_target(&dependency).is_err());
        assert!(ensure_mcp_target(&dependency).is_err());
    }
}
