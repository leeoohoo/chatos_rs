use super::{MediaWorkflow, MediaWorkflowNodeKind};

pub(super) const VIDEO_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(10);
const SUPPORTED_VIDEO_SIZES: [&str; 2] = ["768P", "2K"];
const SUPPORTED_VIDEO_RATIOS: [&str; 6] = ["16:9", "9:16", "1:1", "4:3", "3:4", "21:9"];
const SUPPORTED_VIDEO_DURATION_RANGE: std::ops::RangeInclusive<u16> = 4..=15;
const LEGACY_OPENAI_VIDEO_SIZES: [&str; 4] = ["1280x720", "720x1280", "1792x1024", "1024x1792"];

pub(super) fn default_video_size() -> String {
    "768P".into()
}

pub(super) fn default_video_ratio() -> String {
    "16:9".into()
}

pub(super) fn default_video_seconds() -> u16 {
    4
}

pub(super) fn validate_video_options(size: &str, ratio: &str, seconds: u16) -> bool {
    SUPPORTED_VIDEO_SIZES.contains(&size)
        && SUPPORTED_VIDEO_RATIOS.contains(&ratio)
        && SUPPORTED_VIDEO_DURATION_RANGE.contains(&seconds)
}

pub(super) fn normalize_legacy_workflow_defaults(workflow: &mut MediaWorkflow) {
    for node in &mut workflow.nodes {
        if node.kind != MediaWorkflowNodeKind::Video {
            continue;
        }
        if LEGACY_OPENAI_VIDEO_SIZES.contains(&node.video_size.as_str()) {
            node.video_size = default_video_size();
        }
        if (1..=120).contains(&node.video_seconds)
            && !SUPPORTED_VIDEO_DURATION_RANGE.contains(&node.video_seconds)
        {
            node.video_seconds = node.video_seconds.clamp(
                *SUPPORTED_VIDEO_DURATION_RANGE.start(),
                *SUPPORTED_VIDEO_DURATION_RANGE.end(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{validate_video_options, VIDEO_POLL_INTERVAL};

    #[test]
    fn validates_minimax_h3_workflow_video_options() {
        assert!(validate_video_options("768P", "16:9", 4));
        assert!(!validate_video_options("1280x720", "16:9", 4));
        assert!(!validate_video_options("768P", "2:1", 4));
        assert!(!validate_video_options("768P", "16:9", 16));
    }

    #[test]
    fn keeps_workflow_video_poll_interval_on_the_shared_runtime_contract() {
        assert_eq!(VIDEO_POLL_INTERVAL, std::time::Duration::from_secs(10));
    }
}
