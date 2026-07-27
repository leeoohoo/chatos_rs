// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team
use super::*;
use crate::WorkspaceState;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use uuid::Uuid;

#[test]
fn embedded_catalog_contains_all_expected_skills() {
    let catalog = internal_skill_catalog().expect("catalog");
    assert_eq!(catalog.skills.len(), 28);
    assert_eq!(
        catalog
            .skills
            .iter()
            .filter(|item| item.implementation_status == "ready")
            .count(),
        15
    );
    assert!(catalog.skills.iter().all(|item| {
        !item.name.trim().is_empty()
            && !item.description.trim().is_empty()
            && !item.category.trim().is_empty()
    }));
}

#[test]
fn pdf_release_publishes_bounded_generation_and_editing_tools() {
    let catalog_item = internal_skill_catalog()
        .expect("catalog")
        .skills
        .into_iter()
        .find(|item| item.skill_id == "internal_skill_pdf")
        .expect("PDF catalog item");
    assert_eq!(catalog_item.version, "1.14.0");
    assert_eq!(
        catalog_item.permissions,
        vec!["workspace.read", "workspace.write"]
    );

    let request = serde_json::from_value(json!({
        "type": "skill_prepare_request",
        "request_id": "pdf-tools",
        "owner_user_id": "owner-1",
        "device_id": "device-1",
        "workspace_id": "workspace-1",
        "body": {}
    }))
    .expect("relay request");
    let tools = native::tool_definitions("internal_skill_pdf", &LocalState::default(), &request)
        .expect("PDF tool definitions");
    let names = tools
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .collect::<HashSet<_>>();
    assert_eq!(tools.len(), 16);
    assert!(names.contains("inspect_pdf"));
    assert!(names.contains("extract_pdf_text"));
    assert!(names.contains("render_pdf_pages"));
    assert!(names.contains("export_pdf_pages_to_png"));
    assert!(names.contains("create_text_pdf"));
    assert!(names.contains("create_pdf_from_images"));
    assert!(names.contains("update_pdf_metadata"));
    assert!(names.contains("fill_pdf_form_fields"));
    assert!(names.contains("merge_pdfs"));
    assert!(names.contains("extract_pdf_pages"));
    assert!(names.contains("arrange_pdf_pages"));
    assert!(names.contains("rotate_pdf_pages"));
    assert!(names.contains("add_pdf_text_annotation"));
    assert!(names.contains("stamp_pdf_text"));
    assert!(names.contains("stamp_pdf_page_numbers"));
    assert!(names.contains("stamp_pdf_image"));
    let instructions = internal_skill_instructions("internal_skill_pdf").expect("PDF instructions");
    assert!(instructions.contains("transparent PNG alpha"));
    assert!(instructions.contains("regular non-symlink workspace"));
    assert!(instructions.contains("exact page sequence"));
    assert!(instructions.contains("physical one-based position"));
    assert!(instructions.contains("retain their original physical offset"));
    assert!(instructions.contains("Unicode sticky-note annotations"));
    assert!(instructions.contains("effective page rotation of zero"));
    assert!(instructions.contains("Unicode Document Info inspection and updates"));
    assert!(instructions.contains("semantic no-op fails"));
    assert!(instructions.contains("manifest-verified local PDF page rendering"));
    assert!(instructions.contains("visual_review_status=pending_model_review"));
    assert!(instructions.contains("exact `expected_value`"));
    assert!(instructions.contains("XFA"));
    assert!(instructions.contains("NoToggleToOff"));
    assert!(instructions.contains("exact option order"));
    assert!(instructions.contains("Editable choice fields"));
    assert!(instructions.contains("combined inputs are limited to 100 MiB and 100 megapixels"));
    assert!(instructions.contains("Always call `render_pdf_pages` on the generated PDF"));
    assert!(instructions.contains("target_directory` that does not already exist"));
    assert!(instructions.contains("per-file atomic commit"));
}

#[test]
fn spreadsheets_release_publishes_xlsx_rendering_and_safe_csv_tsv_range_editing_tools() {
    let catalog_item = internal_skill_catalog()
        .expect("catalog")
        .skills
        .into_iter()
        .find(|item| item.skill_id == "internal_skill_spreadsheets")
        .expect("Spreadsheets catalog item");
    assert_eq!(catalog_item.version, "1.4.0");
    let instructions = internal_skill_instructions("internal_skill_spreadsheets")
        .expect("Spreadsheets instructions");
    assert!(instructions.contains("update_xlsx_range"));
    assert!(instructions.contains("never changed in place"));
    assert!(instructions.contains("External-workbook syntax"));
    assert!(instructions.contains("render_spreadsheet_pages"));
    assert!(instructions.contains("visual_review_status=pending_model_review"));
    assert!(instructions.contains("update_csv_range"));
    assert!(instructions.contains("update_tsv_range"));
    assert!(instructions.contains("expected_sha256"));
    assert!(instructions.contains("mixed LF/CRLF"));

    let request = serde_json::from_value(json!({
        "type": "skill_prepare_request",
        "request_id": "spreadsheet-tools",
        "owner_user_id": "owner-1",
        "device_id": "device-1",
        "workspace_id": "workspace-1",
        "body": {}
    }))
    .expect("relay request");
    let tools = native::tool_definitions(
        "internal_skill_spreadsheets",
        &LocalState::default(),
        &request,
    )
    .expect("Spreadsheets tool definitions");
    let names = tools
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .collect::<HashSet<_>>();
    assert_eq!(tools.len(), 8);
    assert!(names.contains("inspect_spreadsheet"));
    assert!(names.contains("render_spreadsheet_pages"));
    assert!(names.contains("create_xlsx"));
    assert!(names.contains("update_xlsx_range"));
    assert!(names.contains("create_csv"));
    assert!(names.contains("update_csv_range"));
    assert!(names.contains("create_tsv"));
    assert!(names.contains("update_tsv_range"));
}

#[test]
fn excel_live_control_release_gates_content_and_number_format_writes_on_approval() {
    let catalog_item = internal_skill_catalog()
        .expect("catalog")
        .skills
        .into_iter()
        .find(|item| item.skill_id == "internal_skill_excel_live_control")
        .expect("Excel Live Control catalog item");
    assert_eq!(catalog_item.version, "1.4.0");
    assert_eq!(catalog_item.implementation_status, "ready");
    assert!(!catalog_item.requires_workspace);
    assert_eq!(catalog_item.permissions, vec!["office.excel.control"]);

    let request = serde_json::from_value(json!({
        "type": "skill_prepare_request",
        "request_id": "excel-live-tools",
        "owner_user_id": "owner-1",
        "device_id": "device-1",
        "workspace_id": "",
        "body": {}
    }))
    .expect("relay request");
    let tools = native::tool_definitions(
        "internal_skill_excel_live_control",
        &LocalState::default(),
        &request,
    )
    .expect("Excel Live Control tool definitions");
    let names = tools
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .collect::<HashSet<_>>();
    assert_eq!(tools.len(), 4);
    assert!(names.contains("excel_live_status"));
    assert!(names.contains("excel_list_open_workbooks"));
    assert!(names.contains("excel_inspect_workbook"));
    assert!(names.contains("excel_read_range"));
    assert!(!native::requires_interactive_approval(
        "internal_skill_excel_live_control",
        "excel_inspect_workbook"
    ));

    let approved_tools = native::plugin_tool_definitions(
        "internal_skill_excel_live_control",
        &LocalState::default(),
        &request,
        false,
        true,
    )
    .expect("approval-gated Excel Live Control tools");
    assert_eq!(approved_tools.len(), 6);
    assert!(approved_tools
        .iter()
        .any(|tool| { tool.get("name").and_then(Value::as_str) == Some("excel_write_range") }));
    assert!(approved_tools.iter().any(|tool| {
        tool.get("name").and_then(Value::as_str) == Some("excel_set_number_format")
    }));
    let unapproved_tools = native::plugin_tool_definitions(
        "internal_skill_excel_live_control",
        &LocalState::default(),
        &request,
        false,
        false,
    )
    .expect("read-only Excel Live Control tools without approval");
    assert_eq!(unapproved_tools.len(), 4);
    assert!(!unapproved_tools
        .iter()
        .any(|tool| { tool.get("name").and_then(Value::as_str) == Some("excel_write_range") }));
    assert!(!unapproved_tools.iter().any(|tool| {
        tool.get("name").and_then(Value::as_str) == Some("excel_set_number_format")
    }));
    assert!(native::requires_interactive_approval(
        "internal_skill_excel_live_control",
        "excel_write_range"
    ));
    assert!(native::requires_interactive_approval(
        "internal_skill_excel_live_control",
        "excel_set_number_format"
    ));

    let instructions = internal_skill_instructions("internal_skill_excel_live_control")
        .expect("Excel Live Control instructions");
    assert!(instructions.contains("approval-gated, exact-snapshot-bound number-format replacement"));
    assert!(instructions.contains("seven fixed presets"));
    assert!(instructions.contains("arbitrary custom format text is not returned"));
    assert!(instructions.contains("never placed in process arguments"));
    assert!(instructions.contains("Never launch, activate, select, close, save, export, reopen"));
    assert!(instructions.contains("exact `worksheet_id`"));
    assert!(instructions.contains("at most 256 cells"));
    assert!(instructions.contains("`range_snapshot_id`"));
    assert!(instructions
        .contains("attempts to restore the exact target contents or exact prior number formats"));
    assert!(instructions.contains("not a workbook transaction"));
    assert!(instructions.contains("does not complete live workbook editing"));
}

#[test]
fn presentations_release_publishes_render_and_existing_edit_contracts() {
    let catalog_item = internal_skill_catalog()
        .expect("catalog")
        .skills
        .into_iter()
        .find(|item| item.skill_id == "internal_skill_presentations")
        .expect("Presentations catalog item");
    assert_eq!(catalog_item.version, "1.24.0");
    let instructions = internal_skill_instructions("internal_skill_presentations")
        .expect("Presentations instructions");
    assert!(instructions.contains("image_right"));
    assert!(instructions.contains("notesMaster/notesSlide"));
    assert!(instructions.contains("Source image files are read-only"));
    assert!(instructions.contains("distinct output file"));
    assert!(instructions.contains("exactly one existing internal notes master"));
    assert!(instructions.contains("replace_pptx_text"));
    assert!(instructions.contains("never guesses across multiple runs"));
    assert!(instructions.contains("replace_pptx_text_across_runs"));
    assert!(instructions.contains("2–16 directly adjacent simple `a:r`"));
    assert!(instructions.contains("must appear exactly once"));
    assert!(instructions.contains("reorder_pptx_slides"));
    assert!(instructions.contains("true visible presentation order"));
    assert!(instructions.contains("`value_axis_log_base`"));
    assert!(instructions.contains("`value_axis_major_tick_mark`"));
    assert!(instructions.contains("`secondary_value_axis_minor_tick_mark`"));
    assert!(instructions.contains("`none`, `inside`, `outside`, or `cross`"));
    assert!(instructions.contains("strictly positive"));
    assert!(instructions.contains("`table` layout"));
    assert!(instructions.contains("1–50 rows and 1–20 string columns"));
    assert!(instructions.contains("immediately eligible for `inspect_pptx_table`"));
    assert!(instructions.contains("inspect_pptx_table"));
    assert!(instructions.contains("replace_pptx_table_cell_text"));
    assert!(instructions.contains("copy_pptx_table_cell_format"));
    assert!(instructions.contains("inspect_pptx_charts"));
    assert!(instructions.contains("replace_pptx_chart"));
    assert!(instructions.contains("byte-identical chart XML"));
    assert!(instructions.contains("expected_self_contained_edit_snapshot"));
    assert!(instructions.contains("workbook is never opened"));
    assert!(instructions.contains("The `chart` layout"));
    assert!(instructions.contains("literal `c:strLit`/`c:numLit` caches"));
    assert!(instructions.contains("Pie and doughnut charts require exactly one series"));
    assert!(instructions.contains("standard 2D `area`"));
    assert!(instructions.contains("fixed 50% hole"));
    assert!(instructions.contains("`legend_position` supports `right`, `left`, `top`, or `bottom`"));
    assert!(instructions.contains("`percentage` only for pie/doughnut"));
    assert!(instructions.contains("optional `category_axis_title` and `value_axis_title`"));
    assert!(instructions.contains("canonical `c:dLbls`"));
    assert!(instructions.contains("literal rich-text category/value-axis titles"));
    assert!(instructions.contains("`value_axis=primary` or `secondary`"));
    assert!(instructions.contains("one same-type group on a hidden top category axis"));
    assert!(instructions.contains("secondary value-axis assignments"));
    assert!(instructions.contains("`value_axis_minimum`/`value_axis_maximum`"));
    assert!(instructions.contains("must include every series value assigned to that axis"));
    assert!(instructions.contains("`value_axis_major_unit`/`value_axis_minor_unit`"));
    assert!(instructions.contains("Every explicit unit must be positive"));
    assert!(instructions.contains("optional `c:logBase` before orientation"));
    assert!(instructions.contains("optional exact `c:majorTickMark`/`c:minorTickMark`"));
    assert!(instructions.contains("optional exact `c:majorUnit`/`c:minorUnit`"));
    assert!(instructions.contains(
        "maximum, log-base, major/minor tick-mark OOXML values, major-unit, and minor-unit values"
    ));
    assert!(instructions.contains("`thousands_2`"));
    assert!(instructions.contains("exact allowlisted `c:numFmt`"));
    assert!(instructions.contains("recognized or custom tick-mark and number-format state"));
    assert!(instructions.contains("edit arbitrary, embedded-workbook"));
    assert!(instructions.contains("complete `cell_xml_sha256` matrix"));
    assert!(instructions.contains("Reference text is never copied"));
    assert!(instructions.contains("delete_pptx_table_row"));
    assert!(instructions.contains("insert_pptx_table_row"));
    assert!(instructions.contains("delete_pptx_table_column"));
    assert!(instructions.contains("insert_pptx_table_column"));
    assert!(instructions.contains("move_pptx_table_row"));
    assert!(instructions.contains("move_pptx_table_column"));
    assert!(instructions.contains("reference_expected_cells"));
    assert!(instructions.contains("preserves every row and cell formatting byte"));
    assert!(instructions.contains("corresponding exact cell XML in every row"));
    assert!(instructions.contains("complete ordered `expected_cells` snapshot"));
    assert!(instructions.contains("preserves the table frame and total row height"));
    assert!(instructions.contains("preserves the table frame and total grid width"));
    assert!(instructions.contains("eligible_for_row_editing=false"));
    assert!(instructions.contains("eligible_for_column_editing=false"));
    assert!(instructions.contains("canonical `<a:tr h=\"...\">` structure"));
    assert!(instructions.contains("canonical `<a:gridCol w=\"...\"/>` structure"));
    assert!(instructions.contains("rectangular physical cell matrix"));
    assert!(instructions.contains("Merged or attributed cells"));
    assert!(instructions.contains("render_presentation_pages"));
    assert!(instructions.contains("pending_model_review"));
    assert!(instructions.contains("external relationship except an inert hyperlink"));
    assert!(instructions.contains("every current slide position exactly once"));
    assert!(instructions.contains("delete_pptx_slides"));
    assert!(instructions.contains("keeping at least one slide"));
    assert!(instructions.contains("custom shows"));
    assert!(instructions.contains("replace_pptx_notes_text"));
    assert!(instructions.contains("uniquely owned standard notesSlide part"));
    assert!(instructions.contains("never creates notes"));
    assert!(instructions.contains("true visible presentation order"));

    let request = serde_json::from_value(json!({
        "type": "skill_prepare_request",
        "request_id": "presentation-tools",
        "owner_user_id": "owner-1",
        "device_id": "device-1",
        "workspace_id": "workspace-1",
        "body": {}
    }))
    .expect("relay request");
    let tools = native::tool_definitions(
        "internal_skill_presentations",
        &LocalState::default(),
        &request,
    )
    .expect("Presentations tool definitions");
    let names = tools
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .collect::<HashSet<_>>();
    assert_eq!(tools.len(), 20);
    assert!(names.contains("inspect_pptx"));
    assert!(names.contains("inspect_pptx_charts"));
    assert!(names.contains("replace_pptx_chart"));
    assert!(names.contains("inspect_pptx_table"));
    assert!(names.contains("render_presentation_pages"));
    assert!(names.contains("create_pptx"));
    assert!(names.contains("append_pptx_slides"));
    assert!(names.contains("reorder_pptx_slides"));
    assert!(names.contains("delete_pptx_slides"));
    assert!(names.contains("replace_pptx_text"));
    assert!(names.contains("replace_pptx_text_across_runs"));
    assert!(names.contains("replace_pptx_table_cell_text"));
    assert!(names.contains("copy_pptx_table_cell_format"));
    assert!(names.contains("delete_pptx_table_row"));
    assert!(names.contains("insert_pptx_table_row"));
    assert!(names.contains("move_pptx_table_row"));
    assert!(names.contains("delete_pptx_table_column"));
    assert!(names.contains("insert_pptx_table_column"));
    assert!(names.contains("move_pptx_table_column"));
    assert!(names.contains("replace_pptx_notes_text"));
    let create = tools
        .iter()
        .find(|tool| tool.get("name").and_then(Value::as_str) == Some("create_pptx"))
        .expect("create_pptx definition");
    assert!(create
        .pointer("/inputSchema/properties/slides/items/properties/layout/enum")
        .and_then(Value::as_array)
        .is_some_and(|layouts| layouts.contains(&json!("table"))));
    assert_eq!(
        create
            .pointer(
                "/inputSchema/properties/slides/items/properties/table/properties/cells/maxItems"
            )
            .and_then(Value::as_u64),
        Some(50)
    );
}

#[test]
fn template_creator_release_publishes_semantic_placeholder_contracts() {
    let catalog_item = internal_skill_catalog()
        .expect("catalog")
        .skills
        .into_iter()
        .find(|item| item.skill_id == "internal_skill_template_creator")
        .expect("Template Creator catalog item");
    assert_eq!(catalog_item.version, "1.2.0");
    let instructions = internal_skill_instructions("internal_skill_template_creator")
        .expect("Template Creator instructions");
    assert!(instructions.contains("{{CLIENT}}"));
    assert!(instructions.contains("tokens split across multiple runs or cells fail closed"));
    assert!(instructions.contains("Legacy schema-v1 templates remain readable"));
    assert!(instructions.contains("render_artifact_template_preview"));
    assert!(instructions.contains("retained template reference"));
    assert!(instructions.contains("visual_review_status=pending_model_review"));

    let request = serde_json::from_value(json!({
        "type": "skill_prepare_request",
        "request_id": "template-tools",
        "owner_user_id": "owner-1",
        "device_id": "device-1",
        "workspace_id": "workspace-1",
        "body": {}
    }))
    .expect("relay request");
    let tools = native::tool_definitions(
        "internal_skill_template_creator",
        &LocalState::default(),
        &request,
    )
    .expect("Template Creator tool definitions");
    let names = tools
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .collect::<HashSet<_>>();
    assert_eq!(tools.len(), 4);
    assert!(names.contains("inspect_artifact_template"));
    assert!(names.contains("create_artifact_template"));
    assert!(names.contains("instantiate_artifact_template"));
    assert!(names.contains("render_artifact_template_preview"));
}

#[test]
fn browser_release_publishes_screencast_route_and_approved_cdp_contracts() {
    let catalog_item = internal_skill_catalog()
        .expect("catalog")
        .skills
        .into_iter()
        .find(|item| item.skill_id == "internal_skill_browser")
        .expect("Browser catalog item");
    assert_eq!(catalog_item.version, "1.8.0");
    let instructions =
        internal_skill_instructions("internal_skill_browser").expect("Browser instructions");
    assert!(instructions.contains("browser_har_start"));
    assert!(instructions.contains("browser_har_stop"));
    assert!(instructions.contains("browser_websocket_start"));
    assert!(instructions.contains("browser_websocket_frames"));
    assert!(instructions.contains("browser_websocket_stop"));
    assert!(instructions.contains("Binary payloads are never returned"));
    assert!(instructions.contains("private temporary directory"));
    assert!(instructions.contains("Page.startScreencast"));
    assert!(instructions.contains("acknowledged immediately"));
    assert!(instructions.contains("bounded long-poll reads"));
    assert!(instructions.contains("never receives the debugger URL"));
    assert!(instructions.contains("Page.printToPDF"));
    assert!(instructions.contains("browser_tabs"));
    assert!(instructions.contains("stable session-scoped IDs"));
    assert!(instructions.contains("last remaining page tab cannot be closed"));
    assert!(instructions.contains("browser_route_add"));
    assert!(instructions.contains("browser_route_list"));
    assert!(instructions.contains("browser_route_remove"));
    assert!(instructions.contains("browser_route_clear"));
    assert!(instructions.contains("browser_cdp_command"));
    assert!(instructions.contains("Every route expires after 30 minutes"));
    assert!(instructions.contains("disabled by default"));
    assert!(native::requires_interactive_approval(
        "internal_skill_browser",
        "browser_route_add"
    ));
    assert!(native::requires_interactive_approval(
        "internal_skill_browser",
        "browser_cdp_command"
    ));
    assert!(!native::requires_interactive_approval(
        "internal_skill_browser",
        "browser_route_clear"
    ));
}

#[test]
fn chrome_release_keeps_existing_session_control_on_the_approval_path() {
    let catalog_item = internal_skill_catalog()
        .expect("catalog")
        .skills
        .into_iter()
        .find(|item| item.skill_id == "internal_skill_chrome")
        .expect("Chrome catalog item");
    assert_eq!(catalog_item.version, "1.4.0");
    assert_eq!(
        catalog_item.permissions,
        [
            "browser.chrome.control",
            "workspace.read",
            "workspace.write"
        ]
    );
    assert!(!catalog_item.requires_workspace);
    let instructions =
        internal_skill_instructions("internal_skill_chrome").expect("Chrome instructions");
    assert!(instructions.contains("existing Google Chrome state"));
    assert!(instructions.contains("macOS and Windows Local Connector"));
    assert!(instructions.contains("HKCU\\Software\\Google\\Chrome"));
    assert!(instructions.contains("user gesture"));
    assert!(instructions.contains("does not read form values"));
    assert!(instructions.contains("currently authorized exact origin"));
    assert!(instructions.contains("short-lived `cr...` target IDs"));
    assert!(instructions.contains("10-MiB regular non-symlink workspace file"));
    assert!(instructions.contains("late extension result"));
    assert!(instructions.contains("chrome_tab_select"));
    assert!(instructions.contains("chrome_tab_scroll"));
    assert!(instructions.contains("chrome_tab_history"));
    assert!(instructions.contains("does not expose a fake keyboard tool"));
    assert!(instructions.contains("chrome_tab_download"));
    assert!(instructions.contains("must not already exist"));
    assert!(instructions.contains("does not request Chrome `downloads` permission"));
    let manifest = internal_skill_manifest("internal_skill_chrome").expect("Chrome manifest");
    assert!(manifest.contains("windows-arm64"));
    assert!(manifest.contains("windows-x64"));
    let request = serde_json::from_value(json!({
        "type": "skill_prepare_request",
        "request_id": "chrome-tools",
        "owner_user_id": "owner-1",
        "device_id": "device-1",
        "workspace_id": "",
        "body": {}
    }))
    .expect("relay request");
    let plugin_tools = native::plugin_tool_definitions(
        "internal_skill_chrome",
        &LocalState::default(),
        &request,
        false,
        true,
    )
    .expect("Chrome tools");
    let names = plugin_tools
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .collect::<HashSet<_>>();
    assert_eq!(plugin_tools.len(), 14);
    assert!(names.contains("chrome_status"));
    assert!(names.contains("chrome_tabs"));
    assert!(names.contains("chrome_tab_snapshot"));
    assert!(names.contains("chrome_tab_navigate"));
    assert!(names.contains("chrome_tab_click"));
    assert!(names.contains("chrome_tab_type_text"));
    assert!(names.contains("chrome_tab_select"));
    assert!(names.contains("chrome_tab_scroll"));
    assert!(names.contains("chrome_tab_history"));
    assert!(names.contains("chrome_tab_activate"));
    assert!(names.contains("chrome_tab_upload"));
    assert!(names.contains("chrome_tab_download"));
    assert!(names.contains("chrome_tab_screenshot"));
    assert!(names.contains("chrome_tab_release"));
    assert!(native::requires_interactive_approval(
        "internal_skill_chrome",
        "chrome_tabs"
    ));
    assert!(native::requires_interactive_approval(
        "internal_skill_chrome",
        "chrome_tab_snapshot"
    ));
    assert!(native::requires_interactive_approval(
        "internal_skill_chrome",
        "chrome_tab_upload"
    ));
    assert!(native::requires_interactive_approval(
        "internal_skill_chrome",
        "chrome_tab_download"
    ));
    assert!(!native::requires_interactive_approval(
        "internal_skill_chrome",
        "chrome_status"
    ));
}

#[test]
fn inventory_never_reports_planned_adapter_as_available() {
    let inventory = local_skill_inventory().expect("inventory");
    assert_eq!(inventory.len(), 28);
    let available_count = inventory
        .iter()
        .filter(|item| item.status == "available")
        .count();
    assert!((12..=15).contains(&available_count));
    let ready_ids = internal_skill_catalog()
        .expect("catalog")
        .skills
        .into_iter()
        .filter(|item| item.implementation_status == "ready")
        .map(|item| item.skill_id)
        .collect::<HashSet<_>>();
    assert!(inventory
        .iter()
        .all(|item| ready_ids.contains(item.skill_id.as_str()) || item.status != "available"));
    assert!(inventory
        .iter()
        .filter(|item| item.status == "available")
        .all(|item| item.dependency_status == "available"));
    assert!(inventory.iter().all(|item| matches!(
        item.dependency_status.as_str(),
        "available" | "missing" | "unsupported" | "error"
    )));
}

#[test]
fn computer_use_release_keeps_control_on_the_plugin_approval_path() {
    let catalog_item = internal_skill_catalog()
        .expect("catalog")
        .skills
        .into_iter()
        .find(|item| item.skill_id == "internal_skill_computer_use")
        .expect("computer use catalog item");
    assert_eq!(catalog_item.implementation_status, "ready");
    assert_eq!(catalog_item.version, "1.19.0");
    assert_eq!(
        catalog_item.permissions,
        vec!["system.accessibility", "desktop.observe", "desktop.control"]
    );

    let inventory_item = local_skill_inventory()
        .expect("inventory")
        .into_iter()
        .find(|item| item.skill_id == "internal_skill_computer_use")
        .expect("computer use inventory item");
    assert_eq!(
        inventory_item.status == "available",
        native::dependency_error("internal_skill_computer_use").is_none()
    );

    let relay_request = serde_json::from_value(json!({
        "type": "skill_prepare_request",
        "request_id": "computer-use-foundation",
        "owner_user_id": "owner-1",
        "device_id": "device-1",
        "workspace_id": "",
        "body": {}
    }))
    .expect("relay request");
    let tools = native::tool_definitions(
        "internal_skill_computer_use",
        &LocalState::default(),
        &relay_request,
    )
    .expect("computer use tool definitions");
    assert_eq!(tools.len(), 7);
    let plugin_tools = native::plugin_tool_definitions(
        "internal_skill_computer_use",
        &LocalState::default(),
        &relay_request,
        true,
        true,
    )
    .expect("approved Plugin Computer Use tool definitions");
    assert_eq!(plugin_tools.len(), 16);
    let instructions = internal_skill_instructions("internal_skill_computer_use")
        .expect("Computer Use instructions");
    assert!(instructions.contains("left-button double-click"));
    assert!(instructions.contains("click_count"));
    assert!(instructions.contains("UI Automation control-view tree"));
    assert!(instructions.contains("writable `ValuePattern`"));
    assert!(instructions.contains("Structured approval audit"));
    assert!(instructions.contains("Text audit cards never contain the text itself"));
    assert!(instructions.contains("automatic_replay_safe=false"));
    assert!(instructions.contains("transient post-action screenshot"));
    assert!(instructions.contains("CONFIRM-XXXXXX"));
    assert!(instructions.contains("single-request length-prefixed stdio"));
    assert!(instructions.contains("same TeamIdentifier"));
    assert!(instructions.contains("AXIsEditable=true"));
    assert!(instructions.contains("TextEditPattern"));
    assert!(instructions.contains("CFEqual"));
    assert!(instructions.contains("Activation recovery contract"));
    assert!(instructions.contains("foreground_changed_after_activation"));
    assert!(instructions.contains("frontmost_application_activation_only"));
    assert!(instructions.contains("computer_capture_frontmost_window"));
    assert!(instructions.contains("identity and geometry"));
    assert!(instructions.contains("computer_set_frontmost_window_bounds"));
    assert!(instructions.contains("AXFullScreen"));
    assert!(instructions.contains("standard Windows window state"));
    assert!(instructions.contains("Frontmost-window geometry and state contract"));
    assert!(instructions.contains("capture only that frontmost window"));
    assert!(instructions.contains("computer_capture_window_layout"));
    assert!(instructions.contains("computer_restore_window_layout"));
    assert!(instructions.contains("multi_window_layout_restore"));
    assert!(instructions.contains("at most 8 ordinary visible top-level windows"));
    assert!(instructions.contains("application_content_rollback=false"));
    assert!(
        instructions.contains("never assumes the controlled window remains on the main display")
    );
    assert!(instructions.contains("No Computer Use action exposes `acceptForSession`"));
    let manifest: Value = serde_json::from_str(include_str!(
        "../../../skill_bundles/internal/computer-use/1.19.0/skill.json"
    ))
    .expect("Computer Use 1.19.0 manifest");
    assert_eq!(
        manifest["platforms"],
        json!(["macos-arm64", "macos-x64", "windows-arm64", "windows-x64"])
    );
    let plugin_tools_without_approval = native::plugin_tool_definitions(
        "internal_skill_computer_use",
        &LocalState::default(),
        &relay_request,
        true,
        false,
    )
    .expect("fail-closed Plugin Computer Use tool definitions");
    assert_eq!(plugin_tools_without_approval.len(), 7);

    let bundle_hash = internal_skill_bundle_hash(&catalog_item);
    let prepare = handle_skill_prepare(
        json!({
            "type": "skill_prepare_request",
            "request_id": "computer-use-permission-gate",
            "owner_user_id": "owner-1",
            "device_id": "device-1",
            "workspace_id": "",
            "body": {
                "skill_id": catalog_item.skill_id,
                "bundle_id": catalog_item.bundle_id,
                "version": catalog_item.version,
                "bundle_hash": bundle_hash,
            }
        }),
        &LocalState::default(),
    );
    let dependency_ready = native::dependency_error("internal_skill_computer_use").is_none();
    assert_eq!(
        prepare["status"].as_u64(),
        Some(if dependency_ready { 200 } else { 409 })
    );
    if !dependency_ready {
        assert!(prepare
            .pointer("/body/error")
            .and_then(Value::as_str)
            .is_some_and(|error| {
                error.contains("Accessibility")
                    || error.contains("Screen Recording")
                    || error.contains("Computer Use helper")
            }));
    }
}

#[test]
fn ready_bundle_v2_fingerprint_matches_plugin_management_seed() {
    let catalog = internal_skill_catalog().expect("catalog");
    let rows = catalog
        .skills
        .iter()
        .filter(|item| item.implementation_status == "ready")
        .map(|item| format!("{}:{}", item.skill_id, internal_skill_bundle_hash(item)))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(
        hex::encode(Sha256::digest(rows.as_bytes())),
        "f7f06221f534a3973bffc12d516c4494e72cffb0b8e9d7b1ffd44560c3f179e1"
    );
}

#[test]
fn all_28_bundled_skill_fingerprints_match_plugin_management_seed() {
    let catalog = internal_skill_catalog().expect("catalog");
    let rows = catalog
        .skills
        .iter()
        .map(|item| format!("{}:{}", item.skill_id, internal_skill_bundle_hash(item)))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(
        hex::encode(Sha256::digest(rows.as_bytes())),
        "d87becf1cc59efc0dae803e69e087a8e4f9d8483b7a9afd908b7c20685a8e2e7"
    );
}

#[test]
fn ready_skill_prepare_returns_local_instructions() {
    let item = internal_skill_catalog()
        .expect("catalog")
        .skills
        .into_iter()
        .find(|item| item.skill_id == "internal_skill_remotion")
        .expect("remotion");
    let request = json!({
        "type": "skill_prepare_request",
        "request_id": "request-1",
        "owner_user_id": "owner-1",
        "device_id": "device-1",
        "workspace_id": "",
        "body": {
            "skill_id": item.skill_id,
            "bundle_id": item.bundle_id,
            "version": item.version,
            "bundle_hash": internal_skill_bundle_hash(&item),
        }
    });
    let response = handle_skill_prepare(request, &LocalState::default());
    assert_eq!(response.get("status").and_then(Value::as_u64), Some(200));
    assert!(response
        .pointer("/body/instructions")
        .and_then(Value::as_str)
        .is_some_and(|value| value.contains("Remotion")));
}

#[test]
fn native_skill_execute_requires_prepared_snapshot_and_writes_locally() {
    let root = std::env::temp_dir().join(format!("chatos-skill-e2e-{}", Uuid::new_v4()));
    fs::create_dir_all(root.as_path()).expect("workspace");
    let state = LocalState {
        workspaces: vec![WorkspaceState {
            id: "workspace-1".to_string(),
            absolute_root: root.clone(),
            alias: "test".to_string(),
            fingerprint: "fp".to_string(),
            project_config_trust: None,
        }],
        ..LocalState::default()
    };
    let item = internal_skill_catalog()
        .expect("catalog")
        .skills
        .into_iter()
        .find(|item| item.skill_id == "internal_skill_visualize")
        .expect("visualize");
    let bundle_hash = internal_skill_bundle_hash(&item);
    let prepare = handle_skill_prepare(
        json!({
            "type": "skill_prepare_request",
            "request_id": "prepare-1",
            "owner_user_id": "owner-1",
            "device_id": "device-1",
            "workspace_id": "workspace-1",
            "body": {
                "skill_id": item.skill_id,
                "bundle_id": item.bundle_id,
                "version": item.version,
                "bundle_hash": bundle_hash,
            }
        }),
        &state,
    );
    assert_eq!(prepare.get("status").and_then(Value::as_u64), Some(200));
    let adapter_session_id = prepare
        .pointer("/body/adapter_session_id")
        .and_then(Value::as_str)
        .expect("adapter session");
    let execute = handle_skill_execute(
        json!({
            "type": "skill_execute_request",
            "request_id": "execute-1",
            "owner_user_id": "owner-1",
            "device_id": "device-1",
            "workspace_id": "workspace-1",
            "body": {
                "skill_id": item.skill_id,
                "bundle_id": item.bundle_id,
                "version": item.version,
                "bundle_hash": bundle_hash,
                "adapter_session_id": adapter_session_id,
                "operation": "write_visualization_html",
                "arguments": {
                    "target_path": "artifacts/e2e.html",
                    "title": "E2E",
                    "body_html": "<main>ready</main>"
                }
            }
        }),
        &state,
    );
    assert_eq!(execute.get("status").and_then(Value::as_u64), Some(200));
    assert!(root.join("artifacts/e2e.html").is_file());
    let cancel = handle_skill_cancel(json!({
        "type": "skill_cancel_request",
        "request_id": "cancel-1",
        "owner_user_id": "owner-1",
        "device_id": "device-1",
        "workspace_id": "workspace-1",
        "body": {
            "skill_id": item.skill_id,
            "bundle_id": item.bundle_id,
            "version": item.version,
            "bundle_hash": bundle_hash,
            "adapter_session_id": adapter_session_id,
        }
    }));
    assert_eq!(cancel.get("status").and_then(Value::as_u64), Some(200));
    assert_eq!(
        cancel.pointer("/body/cancelled").and_then(Value::as_bool),
        Some(true)
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn document_skill_prepare_publishes_and_executes_native_tools() {
    let root = std::env::temp_dir().join(format!("chatos-document-e2e-{}", Uuid::new_v4()));
    fs::create_dir_all(root.as_path()).expect("workspace");
    let state = LocalState {
        workspaces: vec![WorkspaceState {
            id: "workspace-1".to_string(),
            absolute_root: root.clone(),
            alias: "test".to_string(),
            fingerprint: "fp".to_string(),
            project_config_trust: None,
        }],
        ..LocalState::default()
    };
    let item = internal_skill_catalog()
        .expect("catalog")
        .skills
        .into_iter()
        .find(|item| item.skill_id == "internal_skill_documents")
        .expect("documents");
    assert_eq!(item.version, "1.22.0");
    let bundle_hash = internal_skill_bundle_hash(&item);
    let prepare = handle_skill_prepare(
        json!({
            "type": "skill_prepare_request",
            "request_id": "prepare-documents",
            "owner_user_id": "owner-1",
            "device_id": "device-1",
            "workspace_id": "workspace-1",
            "body": {
                "skill_id": item.skill_id,
                "bundle_id": item.bundle_id,
                "version": item.version,
                "bundle_hash": bundle_hash,
            }
        }),
        &state,
    );
    assert_eq!(prepare.get("status").and_then(Value::as_u64), Some(200));
    assert!(prepare
        .pointer("/body/tools")
        .and_then(Value::as_array)
        .is_some_and(|tools| tools
            .iter()
            .any(|tool| { tool.get("name").and_then(Value::as_str) == Some("create_docx") })));
    let document_tool_names = prepare
        .pointer("/body/tools")
        .and_then(Value::as_array)
        .expect("document tools")
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .collect::<HashSet<_>>();
    assert_eq!(document_tool_names.len(), 26);
    assert!(document_tool_names.contains("render_docx_pages"));
    assert!(document_tool_names.contains("update_docx_metadata"));
    assert!(document_tool_names.contains("insert_docx_content_at_paragraph"));
    assert!(document_tool_names.contains("insert_docx_content_at_paragraph_index"));
    assert!(document_tool_names.contains("delete_docx_paragraph"));
    assert!(document_tool_names.contains("delete_docx_paragraph_at_index"));
    assert!(document_tool_names.contains("move_docx_paragraph"));
    assert!(document_tool_names.contains("move_docx_paragraph_at_index"));
    assert!(document_tool_names.contains("replace_docx_paragraph_with_content"));
    assert!(document_tool_names.contains("replace_docx_paragraph_at_index_with_content"));
    assert!(document_tool_names.contains("create_structured_docx"));
    assert!(document_tool_names.contains("append_docx_content"));
    assert!(document_tool_names.contains("replace_docx_text"));
    assert!(document_tool_names.contains("replace_docx_text_across_runs"));
    assert!(document_tool_names.contains("replace_docx_header_footer_text"));
    assert!(document_tool_names.contains("replace_docx_table_cell_text"));
    assert!(document_tool_names.contains("delete_docx_table_row"));
    assert!(document_tool_names.contains("insert_docx_table_row"));
    assert!(document_tool_names.contains("move_docx_table_row"));
    assert!(document_tool_names.contains("insert_docx_image"));
    assert!(document_tool_names.contains("add_docx_header_footer"));
    assert!(document_tool_names.contains("add_docx_comment"));
    assert!(document_tool_names.contains("replace_docx_text_tracked"));
    assert!(document_tool_names.contains("resolve_docx_tracked_changes"));
    let instructions =
        internal_skill_instructions("internal_skill_documents").expect("Documents instructions");
    assert!(instructions.contains("insert_docx_content_at_paragraph"));
    assert!(instructions.contains("insert_docx_content_at_paragraph_index"));
    assert!(instructions.contains("indexed insertion/deletion/movement/replacement eligibility"));
    assert!(instructions.contains("delete_docx_paragraph"));
    assert!(instructions.contains("delete_docx_paragraph_at_index"));
    assert!(instructions.contains("empty paragraphs and repeated paragraph text"));
    assert!(instructions.contains("move_docx_paragraph"));
    assert!(instructions.contains("move_docx_paragraph_at_index"));
    assert!(instructions.contains("Both indices refer to the original inspected paragraph order"));
    assert!(instructions.contains("replace_docx_paragraph_with_content"));
    assert!(instructions.contains("replace_docx_paragraph_at_index_with_content"));
    assert!(instructions.contains("replacement blocks"));
    assert!(instructions.contains("document range markup"));
    assert!(instructions.contains("entire paragraph"));
    assert!(instructions.contains("globally unique eligible top-level paragraph"));
    assert!(instructions.contains("replace_docx_table_cell_text"));
    assert!(instructions.contains("delete_docx_table_row"));
    assert!(instructions.contains("insert_docx_table_row"));
    assert!(instructions.contains("move_docx_table_row"));
    assert!(instructions.contains("expected_cells"));
    assert!(instructions.contains("only row"));
    assert!(instructions.contains("w14:paraId"));
    assert!(instructions.contains("replace_docx_header_footer_text"));
    assert!(instructions.contains("replace_docx_text_across_runs"));
    assert!(instructions.contains("update_docx_metadata"));
    assert!(instructions.contains("standard Unicode DOCX core"));
    assert!(instructions.contains("2–16 directly adjacent simple runs"));
    assert!(instructions.contains("standard document relationships"));
    assert!(instructions.contains("never creates a new header or footer"));
    assert!(instructions.contains("Merged cells"));
    assert!(instructions.contains("manifest-verified LibreOffice runtime"));
    assert!(instructions.contains("visual_review_status=pending_model_review"));
    assert!(instructions.contains("never searches ambient `PATH`"));
    let resolve_tool = prepare
        .pointer("/body/tools")
        .and_then(Value::as_array)
        .and_then(|tools| {
            tools.iter().find(|tool| {
                tool.get("name").and_then(Value::as_str) == Some("resolve_docx_tracked_changes")
            })
        })
        .expect("resolve tracked changes tool");
    assert_eq!(
        resolve_tool
            .pointer("/inputSchema/properties/revision_ids/maxItems")
            .and_then(Value::as_u64),
        Some(1000)
    );
    let adapter_session_id = prepare
        .pointer("/body/adapter_session_id")
        .and_then(Value::as_str)
        .expect("adapter session");
    let execute = handle_skill_execute(
        json!({
            "type": "skill_execute_request",
            "request_id": "execute-documents",
            "owner_user_id": "owner-1",
            "device_id": "device-1",
            "workspace_id": "workspace-1",
            "body": {
                "skill_id": item.skill_id,
                "bundle_id": item.bundle_id,
                "version": item.version,
                "bundle_hash": bundle_hash,
                "adapter_session_id": adapter_session_id,
                "operation": "create_docx",
                "arguments": {
                    "target_path": "artifacts/document.docx",
                    "title": "本机文档",
                    "paragraphs": ["由 Local Connector 创建。"]
                }
            }
        }),
        &state,
    );
    assert_eq!(execute.get("status").and_then(Value::as_u64), Some(200));
    assert!(root.join("artifacts/document.docx").is_file());
    let cancel = handle_skill_cancel(json!({
        "type": "skill_cancel_request",
        "request_id": "cancel-documents",
        "owner_user_id": "owner-1",
        "device_id": "device-1",
        "workspace_id": "workspace-1",
        "body": {
            "skill_id": item.skill_id,
            "bundle_id": item.bundle_id,
            "version": item.version,
            "bundle_hash": bundle_hash,
            "adapter_session_id": adapter_session_id,
        }
    }));
    assert_eq!(
        cancel.pointer("/body/cancelled").and_then(Value::as_bool),
        Some(true)
    );
    let _ = fs::remove_dir_all(root);
}
