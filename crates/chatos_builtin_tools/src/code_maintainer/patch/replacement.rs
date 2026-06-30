pub(super) fn replace_text_once(
    original: &str,
    old_text: &str,
    new_text: &str,
) -> Result<String, String> {
    if old_text.is_empty() {
        return Err("Old text cannot be empty.".to_string());
    }
    let candidates = build_replace_candidates(old_text, new_text, original.contains("\r\n"));
    for (old_candidate, new_candidate) in candidates {
        let positions: Vec<usize> = original
            .match_indices(&old_candidate)
            .map(|(idx, _)| idx)
            .collect();
        if positions.is_empty() {
            continue;
        }
        if positions.len() > 1 {
            return Err(
                "Replacement target matched multiple locations; provide more surrounding old text."
                    .to_string(),
            );
        }
        return Ok(original.replacen(&old_candidate, &new_candidate, 1));
    }
    Err("Replacement target not found in file.".to_string())
}

fn build_replace_candidates(
    old_text: &str,
    new_text: &str,
    use_crlf: bool,
) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut push_unique = |old_candidate: String, new_candidate: String| {
        if out
            .iter()
            .any(|(existing_old, _)| existing_old == &old_candidate)
        {
            return;
        }
        out.push((old_candidate, new_candidate));
    };

    push_unique(old_text.to_string(), new_text.to_string());
    if !old_text.ends_with('\n') {
        push_unique(format!("{old_text}\n"), format!("{new_text}\n"));
    }
    if use_crlf {
        let old_crlf = old_text.replace('\n', "\r\n");
        let new_crlf = new_text.replace('\n', "\r\n");
        push_unique(old_crlf.clone(), new_crlf.clone());
        if !old_crlf.ends_with("\r\n") {
            push_unique(format!("{old_crlf}\r\n"), format!("{new_crlf}\r\n"));
        }
    }
    out
}
