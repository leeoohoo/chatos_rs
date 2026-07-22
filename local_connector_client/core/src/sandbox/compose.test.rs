// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_project_path_cannot_escape_workspace() {
        assert!(normalize_relative_path("services/api").is_ok());
        assert!(normalize_relative_path("../outside").is_err());
        assert!(normalize_relative_path("/tmp/outside").is_err());
    }

    #[test]
    fn compose_rejects_host_control_settings() {
        let request = ComposeUpRequest {
            project_name: "chatos-project".to_string(),
            project_relative_path: None,
            compose_yaml: "services:\n  application:\n    privileged: true\n".to_string(),
            application_dockerfile: Some("FROM alpine\n".to_string()),
            application_dockerfiles: BTreeMap::new(),
            env_file: String::new(),
        };
        let dockerfiles = normalized_application_dockerfiles(&request).expect("dockerfiles");
        assert!(validate_generated_content(&request, &dockerfiles).is_err());
    }

    #[test]
    fn compose_accepts_multiple_program_managed_application_dockerfiles() {
        let application_dockerfiles = BTreeMap::from([
            ("api".to_string(), "FROM node:24\n".to_string()),
            ("worker".to_string(), "FROM python:3.12\n".to_string()),
        ]);
        let request = ComposeUpRequest {
            project_name: "chatos-project".to_string(),
            project_relative_path: None,
            compose_yaml: concat!(
                "services:\n",
                "  api:\n",
                "    build:\n",
                "      context: ../..\n",
                "      dockerfile: .chatos/runtime-environment/services/api/Dockerfile\n",
                "  worker:\n",
                "    build:\n",
                "      context: ../..\n",
                "      dockerfile: .chatos/runtime-environment/services/worker/Dockerfile\n",
            )
            .to_string(),
            application_dockerfile: None,
            application_dockerfiles,
            env_file: String::new(),
        };
        let dockerfiles = normalized_application_dockerfiles(&request).expect("dockerfiles");
        validate_generated_content(&request, &dockerfiles).expect("multi-app compose source");
        assert_eq!(dockerfiles.len(), 2);
    }

    #[test]
    fn compose_rejects_ai_authored_mcp_installation_in_dockerfile() {
        let request = ComposeUpRequest {
            project_name: "chatos-project".to_string(),
            project_relative_path: None,
            compose_yaml: concat!(
                "services:\n",
                "  api:\n",
                "    build:\n",
                "      context: ../..\n",
                "      dockerfile: .chatos/runtime-environment/services/api/Dockerfile\n",
            )
            .to_string(),
            application_dockerfile: None,
            application_dockerfiles: BTreeMap::from([(
                "api".to_string(),
                "FROM node:24\nCOPY chatos-sandbox-mcp-server /opt/chatos/bin/\n".to_string(),
            )]),
            env_file: String::new(),
        };
        assert!(normalized_application_dockerfiles(&request).is_err());
    }

    #[tokio::test]
    async fn writes_each_application_dockerfile_to_managed_service_directory() {
        let runtime_directory = std::env::temp_dir().join(format!(
            "chatos-compose-artifacts-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let dockerfiles = BTreeMap::from([
            ("api".to_string(), "FROM node:24\n".to_string()),
            ("worker".to_string(), "FROM python:3.12\n".to_string()),
        ]);
        write_application_dockerfiles(runtime_directory.as_path(), &dockerfiles)
            .await
            .expect("write application Dockerfiles");
        assert_eq!(
            std::fs::read_to_string(
                runtime_directory
                    .join("services")
                    .join("api")
                    .join("Dockerfile")
            )
            .expect("read api Dockerfile"),
            "FROM node:24\n"
        );
        assert_eq!(
            std::fs::read_to_string(
                runtime_directory
                    .join("services")
                    .join("worker")
                    .join("Dockerfile")
            )
            .expect("read worker Dockerfile"),
            "FROM python:3.12\n"
        );
        let _ = tokio::fs::remove_dir_all(runtime_directory).await;
    }

    #[test]
    fn compose_source_rejects_external_env_files_and_yaml_aliases() {
        assert!(validate_compose_source_before_resolution(
            "services:\n  application:\n    env_file: C:/Users/demo/.env\n"
        )
        .is_err());
        assert!(validate_compose_source_before_resolution(
            "services:\n  application: &base\n    image: alpine\n"
        )
        .is_err());
        assert!(validate_compose_source_before_resolution(
            "services:\n  application:\n    \"env\\u005ffile\": C:/Users/demo/.env\n"
        )
        .is_err());
        assert!(validate_compose_source_before_resolution(
            "services:\n  application:\n    env_file: [.env.chatos]\n"
        )
        .is_ok());
    }

    #[test]
    fn normalized_compose_rejects_bind_mounts_and_public_ports() {
        let project_root = std::env::temp_dir();
        let bind_mount = json!({
            "services": {
                "application": {
                    "volumes": [{
                        "type": "bind",
                        "source": "C:/",
                        "target": "/host"
                    }]
                }
            }
        });
        assert!(validate_normalized_compose(project_root.as_path(), &bind_mount).is_err());

        let public_port = json!({
            "services": {
                "application": {
                    "ports": [{
                        "target": 8080,
                        "published": "8080"
                    }]
                }
            }
        });
        assert!(validate_normalized_compose(project_root.as_path(), &public_port).is_err());
    }

    #[test]
    fn normalized_compose_accepts_named_volumes_and_loopback_ports() {
        let project_root = std::env::temp_dir().join(format!(
            "chatos-compose-test-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let dockerfile = project_root
            .join(RUNTIME_DIRECTORY)
            .join("services")
            .join("application")
            .join("Dockerfile");
        std::fs::create_dir_all(dockerfile.parent().expect("dockerfile parent"))
            .expect("create test runtime directory");
        std::fs::write(dockerfile.as_path(), "FROM alpine\n").expect("write test Dockerfile");
        let project_root = std::fs::canonicalize(project_root).expect("canonical project root");
        let build_context = project_root.to_string_lossy().to_string();
        let compose = json!({
            "services": {
                "application": {
                    "build": {
                        "context": build_context,
                        "dockerfile": ".chatos/runtime-environment/services/application/Dockerfile"
                    },
                    "ports": [{
                        "host_ip": "127.0.0.1",
                        "target": 8080,
                        "published": "8080"
                    }],
                    "volumes": [{
                        "type": "volume",
                        "source": "project-data",
                        "target": "/data"
                    }]
                }
            },
            "volumes": {
                "project-data": {}
            }
        });
        validate_normalized_compose(project_root.as_path(), &compose)
            .expect("safe normalized compose");
        let _ = std::fs::remove_dir_all(project_root);
    }

    #[test]
    fn normalized_compose_service_cannot_reuse_another_services_dockerfile() {
        let project_root = std::env::temp_dir().join(format!(
            "chatos-compose-service-binding-test-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let api_dockerfile = project_root
            .join(RUNTIME_DIRECTORY)
            .join("services")
            .join("api")
            .join("Dockerfile");
        std::fs::create_dir_all(api_dockerfile.parent().expect("api Dockerfile parent"))
            .expect("create managed API directory");
        std::fs::write(api_dockerfile.as_path(), "FROM alpine\n")
            .expect("write managed API Dockerfile");
        let project_root = std::fs::canonicalize(project_root).expect("canonical project root");
        let compose = json!({
            "services": {
                "worker": {
                    "build": {
                        "context": project_root.to_string_lossy(),
                        "dockerfile": ".chatos/runtime-environment/services/api/Dockerfile"
                    }
                }
            }
        });
        assert!(validate_normalized_compose(project_root.as_path(), &compose).is_err());
        let _ = std::fs::remove_dir_all(project_root);
    }

    #[test]
    fn compose_parent_status_is_derived_from_all_child_services() {
        assert_eq!(compose_environment_status(&[]), "stopped");
        assert_eq!(
            compose_environment_status(&[json!({"State": "running"}), json!({"State": "running"})]),
            "running"
        );
        assert_eq!(
            compose_environment_status(&[json!({"State": "running"}), json!({"State": "exited"})]),
            "degraded"
        );
        assert_eq!(
            compose_environment_status(&[json!({"State": "exited"})]),
            "stopped"
        );
        assert_eq!(
            parse_compose_ps(r#"[{"State":"running"},{"State":"exited"}]"#).len(),
            2
        );
    }
}
