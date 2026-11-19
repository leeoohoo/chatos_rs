pub(crate) fn is_generation_only_model(model: &str) -> bool {
    let value = model.trim().to_ascii_lowercase();
    [
        "gpt-image",
        "dall-e",
        "imagen",
        "imagegen",
        "image-generation",
        "sora",
        "veo",
        "kling",
        "seedance",
        "hailuo",
        "runway",
        "luma",
        "pixverse",
        "vidu",
        "cogvideo",
        "hunyuan-video",
    ]
    .iter()
    .any(|candidate| value.contains(candidate))
}
