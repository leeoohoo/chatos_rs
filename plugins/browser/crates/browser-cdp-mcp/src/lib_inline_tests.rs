#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_bounded_and_stable() {
        let catalog = tool_catalog();
        assert!(catalog.len() < 200);
        assert!(serde_json::to_vec(&catalog).unwrap().len() < 512 * 1024);
        let names = catalog
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect::<Vec<_>>();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(names.len(), sorted.len());
    }

    #[test]
    fn every_tool_has_policy_metadata() {
        for tool in tool_catalog() {
            assert!(tool.pointer("/_meta/chatos~1requiredPermissions").is_some());
            assert!(tool.pointer("/_meta/chatos~1riskLevel").is_some());
            assert!(tool.pointer("/_meta/chatos~1approvalMode").is_some());
            assert!(tool.pointer("/_meta/chatos~1timeoutMs").is_some());
            assert!(tool.pointer("/_meta/chatos~1toolResultMaxChars").is_some());
            assert!(tool.pointer("/_meta/chatos~1skillGate/allOf").is_some());
            for path in [
                "/_meta/chatos~1skillGate/evidenceArgument",
                "/_meta/chatos~1skillGate/selectByArgument",
                "/inputSchema/properties/skillEvidence",
            ] {
                assert!(tool.pointer(path).is_none());
            }
            let required = tool
                .pointer("/inputSchema/required")
                .and_then(Value::as_array);
            assert!(required.is_none_or(|items| !items.contains(&json!("skillEvidence"))));
            assert!(matches!(
                tool.pointer("/_meta/chatos~1approvalMode")
                    .and_then(Value::as_str),
                Some("none" | "per_call")
            ));
        }
    }

    #[test]
    fn browser_session_id_is_not_exposed_in_tool_schemas() {
        for tool in tool_catalog() {
            assert!(
                tool.pointer("/inputSchema/properties/browser_session_id")
                    .is_none(),
                "{} still exposes browser_session_id",
                tool["name"]
            );
            assert!(
                tool.pointer("/inputSchema/required")
                    .and_then(Value::as_array)
                    .is_none_or(|required| {
                        required
                            .iter()
                            .all(|name| name.as_str() != Some("browser_session_id"))
                    }),
                "{} still requires browser_session_id",
                tool["name"]
            );
        }
    }

    #[tokio::test]
    async fn browser_session_resolution_prefers_legacy_argument_then_bound_session() {
        let active: ActiveBrowserSession = Arc::new(Mutex::new(Some("bound-session".into())));
        assert_eq!(
            resolve_browser_session_id(&json!({}), &active)
                .await
                .unwrap(),
            "bound-session"
        );
        assert_eq!(
            resolve_browser_session_id(&json!({"browser_session_id":"legacy-session"}), &active)
                .await
                .unwrap(),
            "legacy-session"
        );
    }

    #[tokio::test]
    async fn browser_session_resolution_requires_open_when_unbound() {
        let active: ActiveBrowserSession = Arc::new(Mutex::new(None));
        let error = resolve_browser_session_id(&json!({}), &active)
            .await
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("call browser_session_open first")
        );
    }

    #[test]
    fn public_session_results_hide_internal_id() {
        let mut result = json!({
            "browser_session_id": "bs_private",
            "state": "open",
            "mode": "managed",
            "browser": {"mode": "managed"}
        });
        hide_browser_session_id_from_result(&mut result);
        assert!(result.get("browser_session_id").is_none());
        assert_eq!(result["state"], "open");
        assert_eq!(result["mode"], "managed");
    }

    #[test]
    fn session_open_hides_mode_and_keeps_internal_auto_selection_rules() {
        let tools = tool_catalog();
        let tool = tools
            .iter()
            .find(|tool| tool["name"] == "browser_session_open")
            .unwrap();
        assert_eq!(
            tool.pointer("/_meta/chatos~1approvalMode"),
            Some(&json!("per_call"))
        );
        assert_eq!(
            tool.pointer("/_meta/chatos~1requiredPermissions"),
            Some(&json!([]))
        );
        assert!(tool.pointer("/inputSchema/properties/mode").is_none());
        assert!(tool.pointer("/inputSchema/properties/headless").is_none());
        assert!(
            tool.pointer("/inputSchema/properties/persistent_profile")
                .is_none()
        );
        assert!(
            tools
                .iter()
                .all(|tool| tool["name"] != "browser_session_open_managed")
        );
        assert_eq!(
            tool.pointer("/_meta/chatos~1permissionRules/0/requiredPermissions/0"),
            Some(&json!("browser.managed.launch"))
        );
        assert_eq!(
            tool.pointer("/_meta/chatos~1permissionRules/1/requiredPermissions/0"),
            Some(&json!("browser.chrome.attach"))
        );
        assert!(
            tool.pointer("/inputSchema/properties/executable_path")
                .is_none()
        );
    }

    #[test]
    fn artifact_candidates_are_explicit_and_path_relative() {
        let candidates = artifact_registration_candidates(&json!({
            "artifacts": [{
                "artifact_id": "artifact_local",
                "relative_path": "artifact_local-report.har",
                "display_name": "report.har",
                "media_type": "application/json",
                "size_bytes": 42,
                "sha256": "a".repeat(64)
            }]
        }));
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0]["producer_artifact_id"], "artifact_local");
        assert!(candidates[0].get("absolute_path").is_none());
    }
}
