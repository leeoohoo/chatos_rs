#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> NodeSupplyChainPolicy {
        NodeSupplyChainPolicy {
            baseline_revision: "baseline-2026-08".to_string(),
            dependency_requirements: BTreeMap::from([
                ("react".to_string(), "^19.2.7".to_string()),
                ("vite".to_string(), "^8.1.4".to_string()),
            ]),
            audit_level: "high".to_string(),
            install_script_allowlist: BTreeSet::from(["esbuild".to_string()]),
            install_registry: String::new(),
            audit_registry: String::new(),
        }
    }

    fn evidence_with_manifest() -> SupplyChainEvidenceState {
        SupplyChainEvidenceState {
            node_project_observed: true,
            package_manifest: Some(NodePackageManifestEvidence {
                requirements: BTreeMap::from([
                    ("react".to_string(), "^19.2.7".to_string()),
                    ("vite".to_string(), "^8.1.4".to_string()),
                ]),
            }),
            ..SupplyChainEvidenceState::default()
        }
    }

    fn terminal_result(command: &str, exit_code: i64, output: &str) -> Value {
        json!({
            "name": "sandbox_terminal_controller_execute_command",
            "success": exit_code == 0,
            "is_error": exit_code != 0,
            "result": {
                "common": command,
                "exit_code": exit_code,
                "output": output,
            }
        })
    }

    fn split_terminal_result(command: &str, exit_code: i64, output: &str) -> Value {
        json!({
            "name": "sandbox_terminal_controller_execute_command",
            "success": exit_code == 0,
            "is_error": exit_code != 0,
            "content": serde_json::to_string(&json!({
                "common": command,
                "output": output,
            })).expect("terminal content"),
            "result": {
                "exit_code": exit_code,
                "truncated": false,
            }
        })
    }

    fn background_terminal_start(command: &str, process_id: &str) -> Value {
        json!({
            "name": "sandbox_terminal_controller_execute_command",
            "success": true,
            "is_error": false,
            "result": {
                "background": true,
                "busy": true,
                "common": command,
                "process_id": process_id,
                "output": "",
                "truncated": false
            }
        })
    }

    fn background_terminal_wait(process_id: &str, exit_code: i64, output: &str) -> Value {
        json!({
            "name": "sandbox_terminal_controller_process_wait",
            "success": true,
            "is_error": false,
            "result": {
                "busy": false,
                "completed": true,
                "exit_code": exit_code,
                "process_id": process_id,
                "output": output,
                "truncated": false
            }
        })
    }

    #[test]
    fn clean_audit_passes_with_safe_install_and_lockfile() {
        let mut evidence = evidence_with_manifest();
        evidence.observe_tool_result(&json!({"result": {"changed_files": [{"path": "package.json"}, {"path": "package-lock.json"}]}}));
        evidence.observe_tool_result(&terminal_result("npm ci --ignore-scripts", 0, ""));
        evidence.observe_tool_result(&terminal_result("npm rebuild esbuild", 0, ""));
        evidence.observe_tool_result(&terminal_result(
            "npm audit --audit-level=high --json",
            0,
            r#"{"metadata":{"vulnerabilities":{"total":1,"info":0,"low":1,"moderate":0,"high":0,"critical":0}}}"#,
        ));

        let report = evidence.evaluate(&policy());
        assert_eq!(report.status, "passed");
        assert!(report.blocking_reasons.is_empty());
    }

    #[test]
    fn critical_vulnerability_blocks_success() {
        let mut evidence = evidence_with_manifest();
        evidence.observe_tool_result(&terminal_result("npm ci --ignore-scripts", 0, ""));
        evidence.observe_tool_result(&terminal_result(
            "npm audit --audit-level=high --json",
            1,
            r#"{"metadata":{"vulnerabilities":{"total":1,"info":0,"low":0,"moderate":0,"high":0,"critical":1}}}"#,
        ));

        let report = evidence.evaluate(&policy());
        assert_eq!(report.status, "blocked");
        assert!(report.blocking_reasons[0].contains("critical"));
    }

    #[test]
    fn unavailable_audit_and_unsafe_install_are_not_treated_as_clean() {
        let mut evidence = evidence_with_manifest();
        evidence.observe_tool_result(&terminal_result("npm install", 0, ""));
        evidence.observe_tool_result(&terminal_result(
            "npm audit --audit-level=high --json || true",
            0,
            "network unavailable",
        ));

        let report = evidence.evaluate(&policy());
        assert_eq!(report.status, "blocked");
        assert!(report
            .blocking_reasons
            .iter()
            .any(|reason| reason.contains("outside the approved policy")));
        assert!(report
            .blocking_reasons
            .iter()
            .any(|reason| reason.contains("masked")));
    }

    #[test]
    fn split_terminal_payload_is_merged_before_evaluation() {
        let mut evidence = evidence_with_manifest();
        evidence.observe_tool_result(&split_terminal_result("npm ci --ignore-scripts", 0, ""));
        evidence.observe_tool_result(&split_terminal_result(
            "npm audit --audit-level=high --json",
            0,
            r#"{"metadata":{"vulnerabilities":{"total":0,"info":0,"low":0,"moderate":0,"high":0,"critical":0}}}"#,
        ));

        assert_eq!(evidence.evaluate(&policy()).status, "passed");
    }

    #[test]
    fn background_terminal_wait_supplies_final_audit_evidence() {
        let mut evidence = evidence_with_manifest();
        evidence.observe_tool_result(&terminal_result("npm ci --ignore-scripts", 0, ""));
        evidence.observe_tool_result(&background_terminal_start(
            "npm audit --registry=https://registry.npmjs.org --audit-level=high --json",
            "process-audit",
        ));
        evidence.observe_tool_result(&background_terminal_wait(
            "process-audit",
            0,
            r#"{"metadata":{"vulnerabilities":{"total":0,"info":0,"low":0,"moderate":0,"high":0,"critical":0}}}"#,
        ));

        let report = evidence.evaluate(&policy());
        assert_eq!(report.status, "passed");
        assert_eq!(report.audit_exit_code, Some(0));
        assert_eq!(report.vulnerabilities.expect("audit counts").total, 0);
    }

    #[test]
    fn later_failed_compound_command_does_not_erase_successful_install_or_rebuild() {
        let mut evidence = evidence_with_manifest();
        evidence.observe_tool_result(&terminal_result(
            "npm install --ignore-scripts --no-audit --no-fund",
            0,
            "added packages",
        ));
        evidence.observe_tool_result(&terminal_result("npm rebuild esbuild", 0, "rebuilt"));
        evidence.observe_tool_result(&terminal_result(
            "npm rebuild esbuild && npm run build && npm audit --audit-level=high --json",
            1,
            "audit endpoint unavailable",
        ));
        evidence.observe_tool_result(&terminal_result(
            "npm audit --registry=https://registry.npmjs.org --audit-level=high --json",
            0,
            r#"{"metadata":{"vulnerabilities":{"total":0,"info":0,"low":0,"moderate":0,"high":0,"critical":0}}}"#,
        ));

        let report = evidence.evaluate(&policy());
        assert_eq!(report.status, "passed");
        assert!(report.blocking_reasons.is_empty());
        assert_eq!(report.install_exit_code, Some(0));
    }

    #[test]
    fn failed_later_step_does_not_misclassify_a_confirmed_compound_rebuild() {
        let mut evidence = evidence_with_manifest();
        evidence.observe_tool_result(&terminal_result("npm ci --ignore-scripts", 0, ""));
        evidence.observe_tool_result(&terminal_result(
            "npm rebuild esbuild && npm test && npm run build",
            1,
            "rebuilt dependencies successfully\n\n> test\nfailed test",
        ));
        evidence.observe_tool_result(&terminal_result(
            "npm audit --audit-level=high --json",
            0,
            r#"{"metadata":{"vulnerabilities":{"total":0,"info":0,"low":0,"moderate":0,"high":0,"critical":0}}}"#,
        ));

        let report = evidence.evaluate(&policy());
        assert_eq!(report.status, "passed");
        assert!(report.blocking_reasons.is_empty());
        assert_eq!(
            report.approved_install_script_packages,
            vec!["esbuild".to_string()]
        );
    }

    #[test]
    fn audit_json_is_found_after_a_json_preflight_output() {
        let mut evidence = evidence_with_manifest();
        evidence.observe_tool_result(&terminal_result("npm ci --ignore-scripts", 0, ""));
        evidence.observe_tool_result(&terminal_result(
            "node -e \"console.log(JSON.stringify({ scripts: true }))\" && npm audit --json --audit-level=high",
            0,
            "{\"scripts\":true}\n{\"metadata\":{\"vulnerabilities\":{\"total\":0,\"info\":0,\"low\":0,\"moderate\":0,\"high\":0,\"critical\":0}}}",
        ));

        let report = evidence.evaluate(&policy());
        assert_eq!(report.status, "passed");
        assert_eq!(report.vulnerabilities.expect("audit counts").total, 0);
    }

    #[test]
    fn documentation_checks_do_not_replace_real_node_command_evidence() {
        let mut evidence = evidence_with_manifest();
        evidence.observe_tool_result(&terminal_result("npm ci --ignore-scripts", 0, ""));
        evidence.observe_tool_result(&terminal_result(
            "npm audit --json --audit-level=high",
            0,
            r#"{"metadata":{"vulnerabilities":{"total":0,"info":0,"low":0,"moderate":0,"high":0,"critical":0}}}"#,
        ));
        evidence.observe_tool_result(&terminal_result(
            "grep -q 'npm ci --ignore-scripts' README.md && grep -q 'npm audit --json --audit-level=high' README.md",
            0,
            "",
        ));

        let report = evidence.evaluate(&policy());
        assert_eq!(report.status, "passed");
        assert_eq!(
            report.install_command.as_deref(),
            Some("npm ci --ignore-scripts")
        );
        assert_eq!(
            report.audit_command.as_deref(),
            Some("npm audit --json --audit-level=high")
        );
    }

    #[test]
    fn incomplete_vulnerability_metadata_is_rejected() {
        let mut evidence = evidence_with_manifest();
        evidence.observe_tool_result(&terminal_result("npm ci --ignore-scripts", 0, ""));
        evidence.observe_tool_result(&terminal_result(
            "npm audit --json --audit-level=high",
            0,
            r#"{"metadata":{"vulnerabilities":{"total":0,"low":0,"moderate":0,"high":0,"critical":0}}}"#,
        ));

        let report = evidence.evaluate(&policy());
        assert_eq!(report.status, "blocked");
        assert!(report
            .blocking_reasons
            .iter()
            .any(|reason| reason.contains("complete JSON `metadata.vulnerabilities`")));
    }

    #[test]
    fn failed_or_masked_rebuild_blocks_success() {
        for (command, exit_code) in [
            ("npm rebuild esbuild", 1),
            ("npm rebuild esbuild ||true", 0),
        ] {
            let mut evidence = evidence_with_manifest();
            evidence.observe_tool_result(&terminal_result("npm ci --ignore-scripts", 0, ""));
            evidence.observe_tool_result(&terminal_result(command, exit_code, ""));
            evidence.observe_tool_result(&terminal_result(
                "npm audit --audit-level=high --json",
                0,
                r#"{"metadata":{"vulnerabilities":{"total":0,"info":0,"low":0,"moderate":0,"high":0,"critical":0}}}"#,
            ));

            let report = evidence.evaluate(&policy());
            assert_eq!(report.status, "blocked");
            assert!(report
                .blocking_reasons
                .iter()
                .any(|reason| reason.contains("did not complete successfully")));
        }
    }

    #[test]
    fn package_manifest_write_is_verified_only_after_successful_session_commit() {
        let mut evidence = SupplyChainEvidenceState::default();
        evidence.observe_tool_calls(&json!([{
            "invocation_id": "inv-stage",
            "name": "harness_code_stage_edit_batch",
            "arguments": serde_json::to_string(&json!({
                "session_id": "session-1",
                "operations": [{
                    "kind": "write",
                    "path": "package.json",
                    "content": serde_json::to_string(&json!({
                        "dependencies": {"react": "^19.2.7"},
                        "devDependencies": {"vite": "^8.1.4"}
                    })).expect("manifest")
                }]
            })).expect("arguments")
        }]));
        assert!(evidence.package_manifest.is_none());

        evidence.observe_tool_result(&json!({
            "invocation_id": "inv-stage",
            "name": "harness_code_stage_edit_batch",
            "success": true,
            "is_error": false
        }));
        assert!(evidence.package_manifest.is_none());

        evidence.observe_tool_calls(&json!([{
            "invocation_id": "inv-commit",
            "name": "harness_code_commit_edit_session",
            "arguments": {"session_id": "session-1"}
        }]));
        evidence.observe_tool_result(&json!({
            "invocation_id": "inv-commit",
            "name": "harness_code_commit_edit_session",
            "success": true,
            "is_error": false,
            "result": {"committed_paths": ["package.json"]}
        }));

        assert_eq!(
            evidence
                .package_manifest
                .as_ref()
                .expect("successful manifest")
                .requirements["react"],
            "^19.2.7"
        );
    }

    #[test]
    fn structured_file_read_result_verifies_the_final_package_manifest() {
        let mut evidence = SupplyChainEvidenceState::default();

        // This mirrors the instrumented ToolResult emitted by the MCP runtime:
        // `content` is the model-facing rendering while `result` is the
        // structured CodeMaintainer payload.
        evidence.observe_tool_result(&json!({
            "name": "code_maintainer_read_read_file_raw",
            "success": true,
            "is_error": false,
            "content": "{\"path\":\"package.json\",\"content\":\"{\\\"dependencies\\\":{\\\"react\\\":\\\"^19.2.7\\\"},\\\"devDependencies\\\":{\\\"vite\\\":\\\"^8.1.4\\\"}}\"}",
            "result": {
                "path": "package.json",
                "content": "{\"dependencies\":{\"react\":\"^19.2.7\"},\"devDependencies\":{\"vite\":\"^8.1.4\"}}"
            }
        }));

        assert_eq!(
            evidence
                .package_manifest
                .as_ref()
                .expect("structured file read should verify manifest")
                .requirements["react"],
            "^19.2.7"
        );
    }

    #[test]
    fn nested_structured_file_read_result_verifies_the_final_package_manifest() {
        let mut evidence = SupplyChainEvidenceState::default();

        // This is the exact shape persisted by the cloud MCP runtime.  The
        // ToolResult's `result` contains an outer `_structured_result` wrapper
        // and the model-facing `content` array is present alongside it.
        evidence.observe_tool_result(&json!({
            "name": "code_maintainer_read_read_file_raw",
            "success": true,
            "is_error": false,
            "content": "{\"_structured_result\":{\"path\":\"package.json\",\"content\":\"{\\\"dependencies\\\":{\\\"react\\\":\\\"^19.2.7\\\"},\\\"devDependencies\\\":{\\\"vite\\\":\\\"^8.1.4\\\"}}\"},\"content\":[{\"type\":\"text\",\"text\":\"...\"}]}",
            "result": {
                "_structured_result": {
                    "path": "package.json",
                    "content": "{\"dependencies\":{\"react\":\"^19.2.7\"},\"devDependencies\":{\"vite\":\"^8.1.4\"}}"
                },
                "content": [{"type": "text", "text": "..."}]
            }
        }));

        assert_eq!(
            evidence
                .package_manifest
                .as_ref()
                .expect("nested structured file read should verify manifest")
                .requirements["react"],
            "^19.2.7"
        );
    }

    #[test]
    fn later_partial_manifest_edit_invalidates_baseline_until_final_read() {
        let mut evidence = evidence_with_manifest();
        evidence.observe_tool_calls(&json!([{
            "invocation_id": "inv-stage",
            "name": "harness_code_stage_edit_batch",
            "arguments": {
                "session_id": "session-edit",
                "operations": [{
                    "kind": "replace_text",
                    "path": "package.json",
                    "old_text": "^19.2.7",
                    "new_text": "^18.0.0"
                }]
            }
        }]));
        evidence.observe_tool_result(&json!({
            "invocation_id": "inv-stage",
            "name": "harness_code_stage_edit_batch",
            "success": true,
            "is_error": false
        }));
        assert!(evidence.package_manifest.is_some());
        evidence.observe_tool_calls(&json!([{
            "invocation_id": "inv-commit",
            "name": "harness_code_commit_edit_session",
            "arguments": {"session_id": "session-edit"}
        }]));
        evidence.observe_tool_result(&json!({
            "invocation_id": "inv-commit",
            "name": "harness_code_commit_edit_session",
            "success": true,
            "is_error": false,
            "result": {"committed_paths": ["package.json"]}
        }));
        assert!(evidence.package_manifest.is_none());

        evidence.observe_tool_result(&json!({
            "name": "harness_code_read_file_raw",
            "success": true,
            "is_error": false,
            "content": serde_json::to_string(&json!({
                "path": "package.json",
                "content": serde_json::to_string(&json!({
                    "dependencies": {"react": "^18.0.0"},
                    "devDependencies": {"vite": "^8.1.4"}
                })).expect("manifest")
            })).expect("tool content")
        }));

        let violations = dependency_baseline_violations(
            evidence.package_manifest.as_ref().expect("final manifest"),
            &policy(),
        );
        assert_eq!(violations.len(), 1);
        assert!(violations[0].contains("react"));
    }

    #[test]
    fn aborted_manifest_session_discards_staged_evidence() {
        let mut evidence = evidence_with_manifest();
        evidence.observe_tool_calls(&json!([{
            "invocation_id": "inv-stage",
            "name": "harness_code_stage_edit_batch",
            "arguments": {
                "session_id": "session-abort",
                "operations": [{
                    "kind": "delete",
                    "path": "package.json"
                }]
            }
        }]));
        evidence.observe_tool_result(&json!({
            "invocation_id": "inv-stage",
            "name": "harness_code_stage_edit_batch",
            "success": true,
            "is_error": false
        }));
        assert!(evidence
            .staged_package_manifest_updates
            .contains_key("session-abort"));

        evidence.observe_tool_calls(&json!([{
            "invocation_id": "inv-abort",
            "name": "harness_code_abort_edit_session",
            "arguments": {"session_id": "session-abort"}
        }]));
        evidence.observe_tool_result(&json!({
            "invocation_id": "inv-abort",
            "name": "harness_code_abort_edit_session",
            "success": true,
            "is_error": false
        }));

        assert!(evidence.staged_package_manifest_updates.is_empty());
        assert!(evidence.package_manifest.is_some());
    }

    #[test]
    fn dependency_requirement_mismatch_blocks_supply_chain_success() {
        let mut evidence = evidence_with_manifest();
        evidence
            .package_manifest
            .as_mut()
            .expect("manifest")
            .requirements
            .insert("react".to_string(), "^18.0.0".to_string());
        evidence.observe_tool_result(&terminal_result("npm ci --ignore-scripts", 0, ""));
        evidence.observe_tool_result(&terminal_result(
            "npm audit --audit-level=high --json",
            0,
            r#"{"metadata":{"vulnerabilities":{"total":0,"info":0,"low":0,"moderate":0,"high":0,"critical":0}}}"#,
        ));

        let report = evidence.evaluate(&policy());
        assert_eq!(report.status, "blocked");
        assert!(!report.dependency_baseline_verified);
        assert_eq!(report.dependency_baseline_violations.len(), 1);
        assert!(report.dependency_baseline_violations[0].contains("react"));
        assert!(report.dependency_baseline_violations[0].contains("^19.2.7"));
    }

    #[test]
    fn failed_file_read_does_not_make_supply_chain_gate_applicable() {
        let mut evidence = SupplyChainEvidenceState::default();
        evidence.observe_tool_result(&json!({
            "name": "code_maintainer_read_read_file",
            "success": false,
            "is_error": true,
            "result": { "path": "package.json", "message": "not found" },
        }));

        assert!(!evidence.evaluate(&policy()).applicable);
    }

    #[test]
    fn read_only_node_project_inspection_does_not_make_gate_applicable() {
        let mut evidence = SupplyChainEvidenceState::default();
        evidence.observe_tool_result(&json!({
            "name": "code_maintainer_read_list_dir",
            "success": true,
            "is_error": false,
            "result": {
                "entries": [
                    { "path": "package.json", "type": "file" },
                    { "path": "pnpm-lock.yaml", "type": "file" }
                ]
            }
        }));
        evidence.observe_tool_result(&json!({
            "name": "code_maintainer_read_read_file_raw",
            "success": true,
            "is_error": false,
            "content": serde_json::to_string(&json!({
                "path": "package.json",
                "content": serde_json::to_string(&json!({
                    "dependencies": { "react": "^19.2.7" },
                    "devDependencies": { "vite": "^8.1.4" }
                })).expect("manifest")
            })).expect("tool content")
        }));

        let report = evidence.evaluate(&policy());
        assert!(!report.applicable);
        assert_eq!(report.status, "not_applicable");
    }

    #[test]
    fn committed_dependency_file_change_makes_gate_applicable() {
        let mut evidence = evidence_with_manifest();
        evidence.observe_tool_result(&json!({
            "name": "code_maintainer_write_commit_edit_session",
            "success": true,
            "is_error": false,
            "result": { "committed_paths": [{ "path": "pnpm-lock.yaml" }] }
        }));

        let report = evidence.evaluate(&policy());
        assert!(report.applicable);
        assert_eq!(report.status, "blocked");
        assert!(report
            .blocking_reasons
            .iter()
            .any(|reason| reason.contains("installation was not executed")));
        assert!(report
            .blocking_reasons
            .iter()
            .any(|reason| reason.contains("audit was not executed")));
    }

    #[test]
    fn truncated_audit_output_is_incomplete_evidence() {
        let mut evidence = evidence_with_manifest();
        evidence.observe_tool_result(&terminal_result("npm ci --ignore-scripts", 0, ""));
        let mut audit = terminal_result(
            "npm audit --audit-level=high --json",
            0,
            r#"{"metadata":{"vulnerabilities":{"total":0,"info":0,"low":0,"moderate":0,"high":0,"critical":0}}}"#,
        );
        audit["result"]["truncated"] = json!(true);
        evidence.observe_tool_result(&audit);

        let report = evidence.evaluate(&policy());
        assert_eq!(report.status, "blocked");
        assert!(report
            .blocking_reasons
            .iter()
            .any(|reason| reason.contains("truncated")));
    }

    #[test]
    fn npm_install_and_audit_use_independently_configured_registries() {
        let mut policy = policy();
        policy.install_registry = "https://install.example.test".to_string();
        policy.audit_registry = "https://registry.npmjs.org".to_string();
        let mut evidence = evidence_with_manifest();
        evidence.observe_tool_result(&terminal_result(
            "npm ci --ignore-scripts --registry=https://install.example.test",
            0,
            "",
        ));
        evidence.observe_tool_result(&terminal_result(
            "npm audit --audit-level=high --json --registry=https://registry.npmjs.org",
            0,
            r#"{"metadata":{"vulnerabilities":{"total":0,"info":0,"low":0,"moderate":0,"high":0,"critical":0}}}"#,
        ));

        assert_eq!(evidence.evaluate(&policy).status, "passed");

        policy.audit_registry = "https://audit.example.test".to_string();
        let report = evidence.evaluate(&policy);
        assert_eq!(report.status, "blocked");
        assert!(report
            .blocking_reasons
            .iter()
            .any(|reason| reason.contains("configured audit registry")));
    }

    #[test]
    fn nested_structured_terminal_results_are_archived_as_supply_chain_evidence() {
        let mut evidence = evidence_with_manifest();
        let mut install = terminal_result("npm ci --ignore-scripts", 0, "");
        install["result"] = json!({
            "_structured_result": {
                "_structured_result": install["result"].clone()
            }
        });
        evidence.observe_tool_result(&install);
        let mut audit = terminal_result(
            "npm audit --audit-level=high --json",
            0,
            r#"{"metadata":{"vulnerabilities":{"total":0,"info":0,"low":0,"moderate":0,"high":0,"critical":0}}}"#,
        );
        audit["result"] = json!({
            "_structured_result": {
                "_structured_result": audit["result"].clone()
            }
        });
        evidence.observe_tool_result(&audit);

        let report = evidence.evaluate(&policy());
        assert_eq!(report.status, "passed");
        assert_eq!(report.install_exit_code, Some(0));
        assert_eq!(report.audit_exit_code, Some(0));
    }

    #[test]
    fn passed_receipt_is_inherited_only_at_the_matching_execution_head_and_invalidated_by_changes()
    {
        let policy = policy();
        let mut previous = evidence_with_manifest();
        previous.observe_tool_result(&terminal_result("npm ci --ignore-scripts", 0, ""));
        previous.observe_tool_result(&terminal_result(
            "npm audit --audit-level=high --json",
            0,
            r#"{"metadata":{"vulnerabilities":{"total":0,"info":0,"low":0,"moderate":0,"high":0,"critical":0}}}"#,
        ));
        let previous_report = previous.evaluate(&policy);
        let receipt = previous
            .passed_receipt(&policy, &previous_report)
            .expect("passed receipt");
        let mut run = crate::models::TaskRunRecord::queued(
            "run-current".to_string(),
            "task-current".to_string(),
            "model".to_string(),
            "thread".to_string(),
            json!({
                "resolved_prerequisites": [{
                    "run_id": "run-previous",
                    "execution_group_id": "group-1",
                    "integrated_commit": "head-1",
                    "supply_chain_receipt": receipt,
                }]
            }),
            "2026-08-15T10:00:00Z".to_string(),
        );
        run.workspace_execution = Some(
            serde_json::from_value(json!({
                "status": "ready",
                "execution_group_id": "group-1",
                "execution_base_commit": "head-1"
            }))
            .expect("workspace execution"),
        );

        let mut inherited = SupplyChainEvidenceState::inherit_for_run(&run, &policy)
            .expect("matching receipt should inherit");
        assert_eq!(inherited.evaluate(&policy).status, "passed");

        inherited.observe_tool_result(&json!({
            "name": "code_maintainer_write_commit_edit_session",
            "success": true,
            "is_error": false,
            "result": { "committed_paths": [{ "path": "package-lock.json" }] }
        }));
        let invalidated = inherited.evaluate(&policy);
        assert_eq!(invalidated.status, "blocked");
        assert!(invalidated
            .blocking_reasons
            .iter()
            .any(|reason| reason.contains("installation was not executed")));
        assert!(invalidated
            .blocking_reasons
            .iter()
            .any(|reason| reason.contains("audit was not executed")));

        run.workspace_execution
            .as_mut()
            .expect("workspace")
            .execution_base_commit = Some("different-head".to_string());
        assert!(SupplyChainEvidenceState::inherit_for_run(&run, &policy).is_none());
    }
}
