// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use http::header::AUTHORIZATION;
use http::HeaderMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum BearerTokenError {
    #[error("missing authorization header")]
    MissingAuthorizationHeader,
    #[error("invalid authorization header")]
    InvalidAuthorizationHeader,
    #[error("invalid bearer token")]
    InvalidBearerToken,
}

pub fn bearer_token_from_headers(headers: &HeaderMap) -> Result<&str, BearerTokenError> {
    let value = headers
        .get(AUTHORIZATION)
        .ok_or(BearerTokenError::MissingAuthorizationHeader)?
        .to_str()
        .map_err(|_| BearerTokenError::InvalidAuthorizationHeader)?;
    let mut parts = value.split_whitespace();
    let scheme = parts.next().unwrap_or_default();
    let token = parts.next().unwrap_or_default();
    if !scheme.eq_ignore_ascii_case("Bearer") || token.is_empty() || parts.next().is_some() {
        return Err(BearerTokenError::InvalidBearerToken);
    }
    Ok(token)
}

pub fn query_has_nonempty_parameter(query: Option<&str>, parameter_names: &[&str]) -> bool {
    query
        .into_iter()
        .flat_map(|query| query.split('&'))
        .any(|pair| {
            let mut parts = pair.splitn(2, '=');
            parts
                .next()
                .is_some_and(|key| parameter_names.contains(&key))
                && parts.next().is_some_and(|value| !value.trim().is_empty())
        })
}

#[cfg(test)]
mod tests {
    use http::header::{HeaderValue, AUTHORIZATION};
    use http::HeaderMap;

    use super::{bearer_token_from_headers, query_has_nonempty_parameter, BearerTokenError};

    #[test]
    fn parses_case_insensitive_bearer_token() {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, "bEaReR test-token".parse().unwrap());

        assert_eq!(bearer_token_from_headers(&headers), Ok("test-token"));
    }

    #[test]
    fn classifies_missing_invalid_header_and_invalid_scheme() {
        let mut headers = HeaderMap::new();
        assert_eq!(
            bearer_token_from_headers(&headers),
            Err(BearerTokenError::MissingAuthorizationHeader)
        );

        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_bytes(&[0xff]).expect("opaque header value"),
        );
        assert_eq!(
            bearer_token_from_headers(&headers),
            Err(BearerTokenError::InvalidAuthorizationHeader)
        );

        headers.insert(AUTHORIZATION, "Basic credentials".parse().unwrap());
        assert_eq!(
            bearer_token_from_headers(&headers),
            Err(BearerTokenError::InvalidBearerToken)
        );
    }

    #[test]
    fn rejects_missing_or_ambiguous_token_values() {
        for value in ["Bearer", "Bearer ", "Bearer first second"] {
            let mut headers = HeaderMap::new();
            headers.insert(AUTHORIZATION, value.parse().unwrap());
            assert_eq!(
                bearer_token_from_headers(&headers),
                Err(BearerTokenError::InvalidBearerToken),
                "unexpected result for {value:?}"
            );
        }
    }

    #[test]
    fn detects_only_named_query_parameters_with_nonempty_values() {
        let names = ["access_token", "token"];
        assert!(query_has_nonempty_parameter(
            Some("access_token=token-1"),
            &names
        ));
        assert!(query_has_nonempty_parameter(
            Some("plain=value&token=token-1"),
            &names
        ));
        assert!(!query_has_nonempty_parameter(Some("token="), &names));
        assert!(!query_has_nonempty_parameter(Some("plain=value"), &names));
        assert!(!query_has_nonempty_parameter(None, &names));
    }
}
