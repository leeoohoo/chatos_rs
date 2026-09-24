use chatos_plugin_management_sdk::SkillGateDeclaration;
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
struct Fixture {
    schema_version: u32,
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
struct Case {
    id: String,
    gate: Value,
    arguments: Value,
    #[serde(default)]
    expected_catalog_skills: Vec<String>,
    #[serde(default)]
    expected_required_skills: Vec<String>,
    expected_error: Option<String>,
}

#[test]
fn shared_plugin_skill_gate_fixture_matches_rust_runtime() {
    let fixture: Fixture =
        serde_json::from_str(include_str!("../fixtures/plugin_skill_gate_v1.json"))
            .expect("fixture must be valid JSON");
    assert_eq!(fixture.schema_version, 1);

    for case in fixture.cases {
        let parsed = serde_json::from_value::<SkillGateDeclaration>(case.gate);
        let outcome = match parsed {
            Ok(gate) => gate
                .catalog_skill_names()
                .and_then(|catalog| {
                    gate.required_skill_names(&case.arguments)
                        .map(|required| (catalog, required))
                })
                .map_err(|error| error.code().to_string()),
            Err(_) => Err("invalid_declaration".to_string()),
        };

        if let Some(expected_error) = case.expected_error {
            assert_eq!(
                outcome.expect_err(case.id.as_str()),
                expected_error,
                "fixture case {}",
                case.id
            );
        } else {
            let (catalog, required) = outcome.expect(case.id.as_str());
            assert_eq!(
                catalog, case.expected_catalog_skills,
                "fixture case {}",
                case.id
            );
            assert_eq!(
                required, case.expected_required_skills,
                "fixture case {}",
                case.id
            );
        }
    }
}
