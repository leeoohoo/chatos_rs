pub fn normalize_search_keyword(value: &str) -> String {
    value.trim().to_lowercase()
}

fn compact_search_text(value: &str) -> String {
    value
        .chars()
        .filter(|ch| !matches!(ch, ' ' | '\t' | '\n' | '\r' | '_' | '-' | '.' | '/' | '\\'))
        .collect()
}

fn fuzzy_match(text: &str, keyword: &str) -> bool {
    if keyword.is_empty() {
        return true;
    }
    if text.contains(keyword) {
        return true;
    }

    let mut keyword_iter = keyword.chars();
    let mut current = match keyword_iter.next() {
        Some(ch) => ch,
        None => return true,
    };

    for ch in text.chars() {
        if ch == current {
            match keyword_iter.next() {
                Some(next) => current = next,
                None => return true,
            }
        }
    }

    false
}

pub fn is_search_match(name: &str, relative_path: &str, keyword: &str) -> bool {
    if keyword.is_empty() {
        return true;
    }

    let lower_name = name.to_lowercase();
    let lower_path = relative_path.to_lowercase();

    if fuzzy_match(&lower_name, keyword) || fuzzy_match(&lower_path, keyword) {
        return true;
    }

    let compact_keyword = compact_search_text(keyword);
    if compact_keyword.is_empty() {
        return false;
    }

    let compact_name = compact_search_text(&lower_name);
    let compact_path = compact_search_text(&lower_path);
    fuzzy_match(&compact_name, &compact_keyword) || fuzzy_match(&compact_path, &compact_keyword)
}
