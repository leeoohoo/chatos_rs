use base64::Engine;
use sha2::{Digest, Sha256};

pub fn build_auth_token(user_id: &str, role: &str, secret: &str, ttl_hours: i64) -> String {
    let exp = (chrono::Utc::now() + chrono::Duration::hours(ttl_hours.max(1))).timestamp();
    let payload = format!("{}|{}|{}", user_id, role, exp);
    let sig = sign(payload.as_str(), secret);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(format!("{}|{}", payload, sig))
}

pub fn parse_auth_token(token: &str, secret: &str) -> Option<(String, String, i64)> {
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(token.as_bytes())
        .ok()?;
    let decoded = String::from_utf8(decoded).ok()?;
    let mut parts = decoded.split('|');
    let user_id = parts.next()?.to_string();
    let role = parts.next()?.to_string();
    let exp = parts.next()?.parse::<i64>().ok()?;
    let sig = parts.next()?.to_string();
    if parts.next().is_some() {
        return None;
    }

    let payload = format!("{}|{}|{}", user_id, role, exp);
    if sign(payload.as_str(), secret) != sig {
        return None;
    }
    if chrono::Utc::now().timestamp() > exp {
        return None;
    }
    Some((user_id, role, exp))
}

fn sign(payload: &str, secret: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(payload.as_bytes());
    hasher.update(b"|");
    hasher.update(secret.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::{build_auth_token, parse_auth_token};

    #[test]
    fn roundtrip_build_and_parse() {
        let token = build_auth_token("user-a", "admin", "secret", 1);
        let parsed = parse_auth_token(token.as_str(), "secret");
        assert!(parsed.is_some());
        let (user_id, role, _exp) = parsed.unwrap();
        assert_eq!(user_id, "user-a");
        assert_eq!(role, "admin");
    }

    #[test]
    fn parse_rejects_invalid_secret() {
        let token = build_auth_token("user-a", "admin", "secret", 1);
        let parsed = parse_auth_token(token.as_str(), "wrong-secret");
        assert!(parsed.is_none());
    }
}
