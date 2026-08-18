// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[cfg(test)]
async fn docker_inspect_value(name: &str) -> Result<Value, String> {
    let output = Command::new("docker")
        .arg("inspect")
        .arg(name)
        .output()
        .await
        .map_err(|err| format!("docker inspect failed: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "docker inspect failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    serde_json::from_slice::<Value>(&output.stdout)
        .map_err(|err| format!("parse docker inspect failed: {err}"))?
        .as_array()
        .and_then(|values| values.first())
        .cloned()
        .ok_or_else(|| "docker inspect returned no record".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{NetworkPolicy, ResourceLimits};
    use std::collections::BTreeMap;

    #[test]
    fn normalizes_tcp_docker_host_for_engine_api() {
        assert_eq!(
            docker_api_base_url(Some("tcp://docker-proxy:2375/")),
            Some("http://docker-proxy:2375".to_string())
        );
        assert_eq!(
            docker_api_base_url(Some("https://docker.example.test/")),
            Some("https://docker.example.test".to_string())
        );
        assert_eq!(
            docker_api_base_url(Some("unix:///var/run/docker.sock")),
            None
        );
        assert_eq!(docker_api_base_url(Some("tcp://")), None);
    }

    #[test]
    fn reads_published_agent_port_from_inspect_response() {
        let inspect = json!({
            "NetworkSettings": {
                "Ports": {
                    "49888/tcp": [{"HostIp": "127.0.0.1", "HostPort": "32768"}]
                }
            }
        });
        assert_eq!(
            published_agent_endpoint_from_inspect(&inspect, 49_888, "docker-host"),
            Some("http://docker-host:32768".to_string())
        );
    }

    #[test]
    fn ignores_missing_published_agent_port() {
        assert_eq!(
            published_agent_endpoint_from_inspect(&json!({}), 49_888, "docker-host"),
            None
        );
    }

    #[test]
    fn environment_names_and_labels_follow_compose_project_shape() {
        assert_eq!(
            environment_compose_project_name("environment_ABC"),
            "chatos-environment_abc"
        );
        assert_eq!(
            environment_network_name("environment_ABC"),
            "chatos-environment_abc_default"
        );
        assert_eq!(
            environment_service_name("environment_ABC", "services-api"),
            "chatos-environment_abc-services-api-1"
        );
    }

    #[test]
    fn launcher_stops_agent_when_application_exits_and_vice_versa() {
        let script = program_managed_launcher_script();
        assert!(script.contains("app_pid=$!"));
        assert!(script.contains("agent_pid=$!"));
        assert!(script.contains("kill \"$app_pid\""));
        assert!(script.contains("kill \"$agent_pid\""));
        assert!(script.contains("trap terminate INT TERM HUP"));
    }

    #[test]
    fn docker_sandboxes_enable_an_init_process_for_orphan_reaping() {
        let mut command = Command::new("docker");
        command.arg("run").arg("-d");
        enable_docker_init(&mut command);
        assert_eq!(
            command
                .as_std()
                .get_args()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            vec!["run", "-d", "--init"]
        );

        let mut payload = json!({"HostConfig": {}});
        enable_docker_api_init(&mut payload);
        assert_eq!(payload["HostConfig"]["Init"], json!(true));
    }

    fn postgres_service(environment: BTreeMap<String, String>) -> SandboxEnvironmentServiceSpec {
        SandboxEnvironmentServiceSpec {
            service_id: "postgresql".to_string(),
            service_role: "dependency".to_string(),
            image: "postgres:16-alpine".to_string(),
            dockerfile: None,
            environment,
            mcp_enabled: false,
        }
    }

    #[test]
    fn postgres_dependency_boots_with_migration_role_and_preserves_app_role() {
        let service = postgres_service(BTreeMap::from([
            ("POSTGRES_USER".to_string(), "game_app".to_string()),
            ("POSTGRES_PASSWORD".to_string(), "app-secret".to_string()),
            (
                "POSTGRES_MIGRATION_USER".to_string(),
                "game_migration".to_string(),
            ),
            (
                "POSTGRES_MIGRATION_PASSWORD".to_string(),
                "migration-secret".to_string(),
            ),
        ]));
        let bootstrap = postgres_role_bootstrap(&service)
            .expect("valid roles")
            .expect("separate roles");
        assert_eq!(bootstrap.app_user, "game_app");
        assert_eq!(bootstrap.migration_user, "game_migration");
        let container_environment =
            dependency_container_environment(&service).expect("container environment");
        assert_eq!(
            container_environment
                .get("POSTGRES_USER")
                .map(String::as_str),
            Some("game_migration")
        );
        assert_eq!(
            container_environment
                .get("POSTGRES_PASSWORD")
                .map(String::as_str),
            Some("migration-secret")
        );
    }

    #[test]
    fn postgres_dependency_rejects_one_username_with_two_passwords() {
        let service = postgres_service(BTreeMap::from([
            ("POSTGRES_USER".to_string(), "game".to_string()),
            ("POSTGRES_PASSWORD".to_string(), "app-secret".to_string()),
            ("POSTGRES_MIGRATION_USER".to_string(), "game".to_string()),
            (
                "POSTGRES_MIGRATION_PASSWORD".to_string(),
                "migration-secret".to_string(),
            ),
        ]));
        assert!(postgres_role_bootstrap(&service)
            .expect_err("conflicting credentials")
            .contains("share a username"));
    }

    #[tokio::test]
    #[ignore = "requires Docker and the postgres:16-alpine image"]
    async fn docker_postgres_dependency_provisions_migration_and_app_roles() {
        let mut config = AppConfig::for_tests();
        let environment_id = format!("postgres-smoke-{}", uuid::Uuid::new_v4().simple());
        let work_root = std::env::temp_dir().join(format!("chatos-{environment_id}"));
        let workspace = work_root.join("workspace");
        std::fs::create_dir_all(workspace.as_path()).expect("workspace");
        config.work_root = work_root.clone();
        let backend = DockerSandboxBackend::new(config);
        let service = postgres_service(BTreeMap::from([
            ("POSTGRES_DB".to_string(), "role_smoke".to_string()),
            ("POSTGRES_USER".to_string(), "role_app".to_string()),
            ("POSTGRES_PASSWORD".to_string(), "app-secret".to_string()),
            (
                "POSTGRES_MIGRATION_USER".to_string(),
                "role_migration".to_string(),
            ),
            (
                "POSTGRES_MIGRATION_PASSWORD".to_string(),
                "migration-secret".to_string(),
            ),
        ]));
        let result: Result<(), String> = async {
            backend
                .create_environment(SandboxEnvironmentCreateSpec {
                    environment_id: environment_id.clone(),
                    run_workspace: workspace.to_string_lossy().to_string(),
                    services: vec![service],
                    agent_token: "unused-dependency-token".to_string(),
                    resource_limits: ResourceLimits {
                        cpu: 1.0,
                        memory_mb: 512,
                        disk_mb: 1024,
                        max_processes: 64,
                    },
                    network: NetworkPolicy::default(),
                    effective_permissions:
                        chatos_sandbox_contract::legacy_policy_permission_snapshot(
                            &chatos_sandbox_contract::EffectiveSandboxPolicy::default(),
                            vec![workspace.to_string_lossy().to_string()],
                        ),
                })
                .await?;
            for command in [
                "PGPASSWORD=migration-secret psql -v ON_ERROR_STOP=1 -U role_migration -d role_smoke -c 'CREATE TABLE role_probe (id integer primary key)'",
                "PGPASSWORD=app-secret psql -v ON_ERROR_STOP=1 -U role_app -d role_smoke -c 'INSERT INTO role_probe VALUES (1)'",
                "PGPASSWORD=app-secret psql -v ON_ERROR_STOP=1 -U role_app -d role_smoke -tAc 'SELECT count(*) FROM role_probe' | grep -qx 1",
            ] {
                let exec = backend
                    .exec_environment_service(
                        environment_id.as_str(),
                        "postgresql",
                        &["sh".to_string(), "-ec".to_string(), command.to_string()],
                    )
                    .await?;
                if exec.exit_code != 0 {
                    return Err(format!(
                        "PostgreSQL role smoke command failed: {}",
                        exec.stderr
                    ));
                }
            }
            Ok(())
        }
        .await;
        let cleanup = backend.destroy_environment(environment_id.as_str()).await;
        let _ = std::fs::remove_dir_all(work_root);
        result.expect("PostgreSQL role smoke result");
        cleanup.expect("PostgreSQL role smoke cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn prepares_project_local_playwright_path_for_managed_agent_images() {
        let root = std::env::temp_dir().join(format!(
            "chatos-playwright-link-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(root.as_path()).expect("workspace");
        prepare_managed_playwright_workspace_link(
            root.to_string_lossy().as_ref(),
            "registry.example/chatos-sandbox-agent:dev-node24-python3.11",
        )
        .expect("managed Playwright link");
        let link = root.join(MANAGED_PLAYWRIGHT_WORKSPACE_LINK);
        assert_eq!(
            std::fs::read_link(link.as_path()).expect("read link"),
            Path::new(MANAGED_PLAYWRIGHT_CACHE_TARGET)
        );
        std::fs::remove_dir_all(root).expect("cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn preserves_project_owned_playwright_path() {
        let root = std::env::temp_dir().join(format!(
            "chatos-playwright-owned-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let owned = root.join(MANAGED_PLAYWRIGHT_WORKSPACE_LINK);
        std::fs::create_dir_all(owned.as_path()).expect("owned path");
        std::fs::write(owned.join("marker"), "project-owned").expect("marker");
        prepare_managed_playwright_workspace_link(
            root.to_string_lossy().as_ref(),
            "chatos-sandbox-agent:dev-node24-python3.11",
        )
        .expect("preserve owned path");
        assert_eq!(
            std::fs::read_to_string(owned.join("marker")).expect("marker read"),
            "project-owned"
        );
        std::fs::remove_dir_all(root).expect("cleanup");
    }

    #[tokio::test]
    #[ignore = "requires Docker and a locally built chatos-sandbox-agent:latest image"]
    async fn docker_environment_smoke_groups_services_and_keeps_dependency_agent_free() {
        let mut config = AppConfig::from_env().expect("config");
        config.docker_image = "chatos-sandbox-agent:latest".to_string();
        let environment_id = format!("smoke-{}", uuid::Uuid::new_v4().simple());
        let work_root = std::env::temp_dir().join(format!("chatos-{environment_id}"));
        let workspace = work_root.join("workspace");
        std::fs::create_dir_all(workspace.as_path()).expect("workspace");
        std::fs::write(workspace.join("README.md"), "smoke\n").expect("source");
        config.work_root = work_root.clone();
        let backend = DockerSandboxBackend::new(config.clone());
        let mut dependency_environment = BTreeMap::new();
        dependency_environment.insert(
            "MONGO_INITDB_ROOT_USERNAME".to_string(),
            "smoke-user".to_string(),
        );
        dependency_environment.insert(
            "MONGO_INITDB_ROOT_PASSWORD".to_string(),
            "smoke-secret".to_string(),
        );
        dependency_environment.insert("MONGO_INITDB_DATABASE".to_string(), "smoke".to_string());
        let result: Result<(), String> = async {
            let instance = backend
                .create_environment(SandboxEnvironmentCreateSpec {
                    environment_id: environment_id.clone(),
                    run_workspace: workspace.to_string_lossy().to_string(),
                    services: vec![
                        SandboxEnvironmentServiceSpec {
                            service_id: "mongodb".to_string(),
                            service_role: "dependency".to_string(),
                            image: "mongo:7.0".to_string(),
                            dockerfile: None,
                            environment: dependency_environment,
                            mcp_enabled: false,
                        },
                        SandboxEnvironmentServiceSpec {
                            service_id: "api".to_string(),
                            service_role: "application".to_string(),
                            image: config.docker_image.clone(),
                            dockerfile: Some(
                                "FROM mongo:7.0\nENTRYPOINT []\nCMD [\"sh\", \"-c\", \"while true; do sleep 3600; done\"]\n"
                                    .to_string(),
                            ),
                            environment: BTreeMap::new(),
                            mcp_enabled: true,
                        },
                    ],
                    agent_token: "smoke-program-token".to_string(),
                    resource_limits: ResourceLimits {
                        cpu: 1.0,
                        memory_mb: 512,
                        disk_mb: 1024,
                        max_processes: 64,
                    },
                    network: NetworkPolicy::default(),
                    effective_permissions:
                        chatos_sandbox_contract::legacy_policy_permission_snapshot(
                            &chatos_sandbox_contract::EffectiveSandboxPolicy::default(),
                            vec![workspace.to_string_lossy().to_string()],
                        ),
                })
                .await?;
            let dependency = instance
                .services
                .iter()
                .find(|service| service.service_id == "mongodb")
                .ok_or_else(|| "mongodb service missing".to_string())?;
            if dependency.agent_endpoint.is_some() {
                return Err("dependency unexpectedly received an Agent endpoint".to_string());
            }
            let application = instance
                .services
                .iter()
                .find(|service| service.service_id == "api")
                .ok_or_else(|| "api service missing".to_string())?;
            if application.agent_endpoint.is_none() {
                return Err("application Agent endpoint is missing".to_string());
            }
            let dependency_inspect = docker_inspect_value(
                environment_service_name(environment_id.as_str(), "mongodb").as_str(),
            )
            .await?;
            let dependency_env = json_string_array(dependency_inspect.pointer("/Config/Env"));
            if dependency_env
                .iter()
                .any(|value| value.starts_with("CHATOS_SANDBOX_MCP_"))
            {
                return Err("dependency received program-managed MCP environment".to_string());
            }
            let compose_project = dependency_inspect
                .pointer("/Config/Labels/com.docker.compose.project")
                .and_then(Value::as_str);
            if compose_project != Some(environment_compose_project_name(&environment_id).as_str())
            {
                return Err("Compose parent label is missing".to_string());
            }
            let exec = backend
                .exec_environment_service(
                    environment_id.as_str(),
                    "api",
                    &[
                        "sh".to_string(),
                        "-c".to_string(),
                        "test -x /usr/local/bin/chatos-sandbox-mcp-server".to_string(),
                    ],
                )
                .await?;
            if exec.exit_code != 0 {
                return Err(format!("application Agent binary check failed: {}", exec.stderr));
            }
            backend.stop_environment(environment_id.as_str()).await?;
            backend.start_environment(environment_id.as_str()).await?;
            Ok(())
        }
        .await;
        let cleanup = backend.destroy_environment(environment_id.as_str()).await;
        let _ = std::fs::remove_dir_all(work_root);
        result.expect("Docker environment smoke result");
        cleanup.expect("Docker environment smoke cleanup");
    }
}
