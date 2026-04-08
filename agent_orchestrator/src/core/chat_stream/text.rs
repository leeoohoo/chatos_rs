pub(super) fn join_stream_text(current: &str, chunk: &str) -> String {
    if chunk.is_empty() {
        return current.to_string();
    }
    if current.is_empty() {
        return chunk.to_string();
    }

    if chunk.starts_with(current) {
        return chunk.to_string();
    }
    if current.starts_with(chunk) {
        return current.to_string();
    }

    let max_overlap = std::cmp::min(current.len(), chunk.len());
    for overlap in (8..=max_overlap).rev() {
        let Some(current_tail) = current.get(current.len() - overlap..) else {
            continue;
        };
        let Some(chunk_head) = chunk.get(..overlap) else {
            continue;
        };
        if current_tail == chunk_head {
            let rest = chunk.get(overlap..).unwrap_or_default();
            return format!("{}{}", current, rest);
        }
    }

    format!("{}{}", current, chunk)
}
