// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Deserialize;
use std::sync::LazyLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelWorkload {
    Chat,
    Image,
    Video,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Catalog {
    image_prefixes: Vec<String>,
    video_prefixes: Vec<String>,
}

static CATALOG: LazyLock<Catalog> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../clients/macos/Sources/ChatOSModelCapabilities/Resources/ModelWorkloads.json"
    ))
    .expect("valid shared model workload catalog")
});

impl ModelWorkload {
    pub fn for_model(name: &str) -> Self {
        let name = name.trim().to_ascii_lowercase();
        if CATALOG
            .image_prefixes
            .iter()
            .any(|prefix| name.starts_with(prefix))
        {
            Self::Image
        } else if CATALOG
            .video_prefixes
            .iter()
            .any(|prefix| name.starts_with(prefix))
        {
            Self::Video
        } else {
            Self::Chat
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workloads_do_not_confuse_image_inputs_with_generation() {
        for model in ["gpt-6.1-sol", "deepseek-v4-pro", "MiniMax-M2"] {
            assert_eq!(ModelWorkload::for_model(model), ModelWorkload::Chat);
        }
        for model in ["gpt-image-2", "gpt-image-2.5-sunburst", "dall-e-3"] {
            assert_eq!(ModelWorkload::for_model(model), ModelWorkload::Image);
        }
        assert_eq!(
            ModelWorkload::for_model(" MiniMax-H3 "),
            ModelWorkload::Video
        );
    }
}
