use super::*;

#[test]
fn parses_router_metadata_and_dependency_lists() {
    let parsed = parse_skill_document(
        r#"---
name: diagram-studio
description: Route diagram work to one focused diagram workflow.
metadata:
  chatos.role: router
  chatos.required-skills: "diagram-architecture, diagram-sequence"
  chatos.related-skills:
    - diagram-flowchart
    - diagram-sequence
  chatos.context-mode: inline
  chatos.max-output-chars: 12000
---

# Diagram Studio

Activate only the workflow needed for the requested diagram.
"#,
        Some("diagram-studio"),
    )
    .unwrap();

    assert_eq!(parsed.metadata.role, SkillRole::Router);
    assert_eq!(parsed.metadata.context_mode, SkillContextMode::Inline);
    assert_eq!(parsed.metadata.max_output_chars, Some(12_000));
    assert_eq!(
        parsed.metadata.required_skills,
        ["diagram-architecture", "diagram-sequence"]
    );
    assert_eq!(
        parsed.metadata.related_skills,
        ["diagram-flowchart", "diagram-sequence"]
    );
    assert!(parsed.instructions.starts_with("# Diagram Studio"));
}

#[test]
fn disable_model_invocation_overrides_activation_metadata() {
    let parsed = parse_skill_document(
        r#"---
name: explicit-review
description: Run an explicitly requested review.
disable-model-invocation: true
metadata:
  chatos.activation-policy: model-only
---
# Review
Only run when selected by the user.
"#,
        Some("explicit-review"),
    )
    .unwrap();
    assert_eq!(
        parsed.metadata.activation_policy,
        SkillActivationPolicy::UserOnly
    );
}

#[test]
fn rejects_directory_name_mismatch() {
    let error = parse_skill_document(
        "---\nname: wrong-name\ndescription: A useful workflow.\n---\n# Body",
        Some("expected-name"),
    )
    .unwrap_err();
    assert!(error
        .to_string()
        .contains("must match Skill directory name"));
}

#[test]
fn rejects_missing_or_unterminated_frontmatter() {
    assert_eq!(
        parse_skill_document("# Missing", None).unwrap_err(),
        SkillDocumentError::MissingFrontmatter
    );
    assert_eq!(
        parse_skill_document("---\nname: missing-end", None).unwrap_err(),
        SkillDocumentError::UnterminatedFrontmatter
    );
}
