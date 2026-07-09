// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use super::local_connector::{push_local_connector_maven_targets, sort_local_connector_targets};
use super::*;

fn local_connector_project() -> Project {
    Project::new(
        "vrad-backend".to_string(),
        "local://connector/device/workspace/zj/ewo/vrad-backend".to_string(),
        None,
        None,
        Some("user_1".to_string()),
    )
}

#[test]
fn local_connector_maven_spring_boot_targets_use_pom_manifest() {
    let project = local_connector_project();
    let root_entries = HashSet::from(["pom.xml".to_string()]);
    let pom = r#"
        <project>
          <parent>
            <groupId>org.springframework.boot</groupId>
            <artifactId>spring-boot-starter-parent</artifactId>
          </parent>
          <properties>
            <mainClass>com.example.VradApplication</mainClass>
          </properties>
        </project>
    "#;
    let mut targets = Vec::new();

    push_local_connector_maven_targets(&project, &root_entries, Some(pom), &mut targets);
    sort_local_connector_targets(&mut targets);

    let commands = targets
        .iter()
        .map(|target| target.command.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        commands,
        vec![
            "mvn -Dspring-boot.run.main-class=com.example.VradApplication spring-boot:run",
            "mvn test",
        ]
    );
    assert_eq!(targets[0].kind, "java");
    assert_eq!(targets[0].source, "local_connector_maven");
    assert_eq!(
        targets[0].manifest_path.as_deref(),
        Some("local://connector/device/workspace/zj/ewo/vrad-backend/pom.xml")
    );
    assert!(targets[0].required_toolchains.is_empty());
}
