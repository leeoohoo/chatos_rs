// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub(super) struct BundledPluginSpec {
    pub(super) name: &'static str,
    pub(super) display_name: &'static str,
    pub(super) description: &'static str,
    pub(super) category: &'static str,
    pub(super) skill_ids: &'static [&'static str],
    pub(super) release_version: &'static str,
    pub(super) release_epoch: &'static str,
    pub(super) artifact_revision: &'static str,
}

pub(super) const BUNDLED_DEFAULT_RELEASE_VERSION: &str = "1.0.0";
pub(super) const BUNDLED_RELEASE_EPOCH: &str = "2026-07-22T00:00:00Z";
pub(super) const BUNDLED_INITIAL_ARTIFACT_REVISION: &str = "2026-07-13.5";

const fn bundled_plugin(
    name: &'static str,
    display_name: &'static str,
    description: &'static str,
    category: &'static str,
    skill_ids: &'static [&'static str],
) -> BundledPluginSpec {
    bundled_plugin_release(
        name,
        display_name,
        description,
        category,
        skill_ids,
        BUNDLED_DEFAULT_RELEASE_VERSION,
        BUNDLED_RELEASE_EPOCH,
        BUNDLED_INITIAL_ARTIFACT_REVISION,
    )
}

const fn bundled_plugin_release(
    name: &'static str,
    display_name: &'static str,
    description: &'static str,
    category: &'static str,
    skill_ids: &'static [&'static str],
    release_version: &'static str,
    release_epoch: &'static str,
    artifact_revision: &'static str,
) -> BundledPluginSpec {
    BundledPluginSpec {
        name,
        display_name,
        description,
        category,
        skill_ids,
        release_version,
        release_epoch,
        artifact_revision,
    }
}

pub(super) fn bundled_plugin_specs() -> &'static [BundledPluginSpec] {
    &BUNDLED_PLUGIN_SPECS
}

static BUNDLED_PLUGIN_SPECS: [BundledPluginSpec; 12] = [
    bundled_plugin_release(
        "documents",
        "Documents",
        "Create, edit, render, and verify document artifacts.",
        "Productivity",
        &["internal_skill_documents"],
        "1.22.0",
        "2026-07-25T16:00:00Z",
        "documents-1.22.0",
    ),
    bundled_plugin_release(
        "pdf",
        "PDF",
        "Read, create, inspect, render, and verify PDF artifacts.",
        "Productivity",
        &["internal_skill_pdf"],
        "1.17.0",
        "2026-07-27T18:00:00Z",
        "pdf-1.17.0",
    ),
    bundled_plugin_release(
        "spreadsheets",
        "Spreadsheets",
        "Create and operate spreadsheet workbooks and live Excel sessions.",
        "Productivity",
        &[
            "internal_skill_spreadsheets",
            "internal_skill_excel_live_control",
        ],
        "1.8.0",
        "2026-07-26T22:00:00Z",
        "spreadsheets-1.8.0",
    ),
    bundled_plugin_release(
        "presentations",
        "Presentations",
        "Create and edit presentation decks with visual verification.",
        "Productivity",
        &["internal_skill_presentations"],
        "1.24.0",
        "2026-07-26T13:00:00Z",
        "presentations-1.24.0",
    ),
    bundled_plugin_release(
        "template-creator",
        "Template Creator",
        "Create reusable artifact templates from existing files.",
        "Productivity",
        &["internal_skill_template_creator"],
        "1.2.0",
        "2026-07-25T20:00:00Z",
        "template-creator-1.2.0",
    ),
    bundled_plugin(
        "remotion",
        "Remotion",
        "Build video compositions with reviewed Remotion practices.",
        "Creativity",
        &["internal_skill_remotion"],
    ),
    bundled_plugin(
        "figma",
        "Figma",
        "Design-to-code, diagrams, libraries, motion, FigJam, and Slides workflows.",
        "Creativity",
        &[
            "internal_skill_figma_code_connect",
            "internal_skill_figma_create_new_file",
            "internal_skill_figma_design_to_code",
            "internal_skill_figma_generate_design",
            "internal_skill_figma_generate_diagram",
            "internal_skill_figma_generate_library",
            "internal_skill_figma_implement_motion",
            "internal_skill_figma_swiftui",
            "internal_skill_figma_use",
            "internal_skill_figma_use_figjam",
            "internal_skill_figma_use_motion",
            "internal_skill_figma_use_slides",
        ],
    ),
    bundled_plugin_release(
        "browser",
        "Browser",
        "Control the in-app browser for interactive web workflows.",
        "Automation",
        &["internal_skill_browser"],
        "1.8.0",
        "2026-07-24T02:00:00Z",
        "browser-1.8.0",
    ),
    bundled_plugin_release(
        "chrome",
        "Chrome",
        "Connect and narrowly control explicitly authorized tabs from the user's existing Chrome session.",
        "Automation",
        &["internal_skill_chrome"],
        "1.4.0",
        "2026-07-25T07:00:00Z",
        "chrome-1.4.0",
    ),
    bundled_plugin_release(
        "computer-use",
        "Computer Use",
        "Observe and control macOS or Windows desktops with exact native identity binding. Adds a volatile 10-minute opaque snapshot and one-time restore for at most 8 ordinary windows; restore accepts only snapshot ID/SHA-256, requires fresh typed confirmation, and fails closed on display, process, native-window, state, or capability drift.",
        "Automation",
        &["internal_skill_computer_use"],
        "1.19.0",
        "2026-07-27T15:00:00Z",
        "computer-use-1.19.0",
    ),
    bundled_plugin(
        "visualize",
        "Visualize",
        "Create interactive visualizations and exploration tools.",
        "Creativity",
        &["internal_skill_visualize"],
    ),
    bundled_plugin(
        "chatos-developer-kit",
        "ChatOS Developer Kit",
        "OpenAI documentation, Plugin and Skill authoring, installation, and image generation.",
        "Developer Tools",
        &[
            "internal_skill_openai_docs",
            "internal_skill_plugin_creator",
            "internal_skill_skill_creator",
            "internal_skill_skill_installer",
            "internal_skill_imagegen",
        ],
    ),
];
