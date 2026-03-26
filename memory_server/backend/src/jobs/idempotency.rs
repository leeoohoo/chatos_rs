use sha2::{Digest, Sha256};

pub(crate) fn digest_from_ids(namespace: &str, ids: &[String]) -> Option<String> {
    let mut hasher = Sha256::new();
    hasher.update(namespace.trim().as_bytes());
    hasher.update(b"\n");

    let mut count = 0usize;
    for id in ids {
        let normalized = id.trim();
        if normalized.is_empty() {
            continue;
        }
        hasher.update(normalized.as_bytes());
        hasher.update(b"\n");
        count += 1;
    }

    if count == 0 {
        return None;
    }

    let digest = hasher.finalize();
    Some(format!("sha256:{:x}", digest))
}
