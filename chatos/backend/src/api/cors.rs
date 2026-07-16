// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::{
    header::{
        HeaderName, HeaderValue, ACCEPT, ACCESS_CONTROL_REQUEST_HEADERS, AUTHORIZATION,
        CONTENT_TYPE, ORIGIN,
    },
    request::Parts,
};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};

use crate::core::project_execution::{
    CHATOS_CLIENT_SURFACE_COMPAT_HEADER, CHATOS_CLIENT_SURFACE_HEADER,
    LOCAL_CONNECTOR_DESKTOP_SURFACE,
};

pub(super) fn layer(configured_origins: &[String], exposed_header: HeaderName) -> CorsLayer {
    let cors = CorsLayer::new()
        .allow_headers(allowed_headers())
        .expose_headers([exposed_header])
        .allow_methods(Any);

    if configured_origins.iter().any(|origin| origin == "*") {
        return cors.allow_origin(Any).allow_credentials(false);
    }

    cors.allow_origin(desktop_aware_origins(configured_origins))
        .allow_credentials(true)
}

fn allowed_headers() -> Vec<HeaderName> {
    vec![
        ACCEPT,
        AUTHORIZATION,
        CONTENT_TYPE,
        ORIGIN,
        HeaderName::from_static(CHATOS_CLIENT_SURFACE_COMPAT_HEADER),
        HeaderName::from_static("x-api-key"),
        HeaderName::from_static("x-openai-key"),
        HeaderName::from_static("x-user-id"),
        HeaderName::from_static("x-project-id"),
        HeaderName::from_static("x-conversation-id"),
        HeaderName::from_static("x-request-id"),
        HeaderName::from_static("x-remote-verification-code"),
        HeaderName::from_static(CHATOS_CLIENT_SURFACE_HEADER),
    ]
}

fn desktop_aware_origins(configured_origins: &[String]) -> AllowOrigin {
    let configured_origins = configured_origins
        .iter()
        .filter_map(|origin| origin.parse::<HeaderValue>().ok())
        .collect::<Vec<_>>();

    AllowOrigin::predicate(move |origin, request| {
        origin_is_allowed(&configured_origins, origin, request)
    })
}

fn origin_is_allowed(
    configured_origins: &[HeaderValue],
    origin: &HeaderValue,
    request: &Parts,
) -> bool {
    configured_origins.contains(origin)
        || (is_bundled_desktop_origin(origin) && request_has_desktop_surface(request))
}

fn is_bundled_desktop_origin(origin: &HeaderValue) -> bool {
    let Ok(origin) = origin.to_str() else {
        return false;
    };
    let Ok(url) = url::Url::parse(origin) else {
        return false;
    };

    url.scheme() == "http"
        && url.host_str() == Some("127.0.0.1")
        && url.port().is_some()
        && url.path() == "/"
        && url.query().is_none()
        && url.fragment().is_none()
        && url.username().is_empty()
        && url.password().is_none()
}

fn request_has_desktop_surface(request: &Parts) -> bool {
    let surface_headers = [
        CHATOS_CLIENT_SURFACE_HEADER,
        CHATOS_CLIENT_SURFACE_COMPAT_HEADER,
    ];
    let has_actual_header = surface_headers.iter().any(|header| {
        request
            .headers
            .get(*header)
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .is_some_and(|value| value.eq_ignore_ascii_case(LOCAL_CONNECTOR_DESKTOP_SURFACE))
    });
    if has_actual_header {
        return true;
    }

    request
        .headers
        .get(ACCESS_CONTROL_REQUEST_HEADERS)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|headers| {
            headers.split(',').map(str::trim).any(|header| {
                surface_headers
                    .iter()
                    .any(|surface| header.eq_ignore_ascii_case(surface))
            })
        })
}

#[cfg(test)]
mod tests {
    use axum::http::{header::HeaderValue, Request};

    use super::{
        allowed_headers, is_bundled_desktop_origin, origin_is_allowed, request_has_desktop_surface,
    };
    use crate::core::project_execution::{
        CHATOS_CLIENT_SURFACE_COMPAT_HEADER, CHATOS_CLIENT_SURFACE_HEADER,
        LOCAL_CONNECTOR_DESKTOP_SURFACE,
    };

    fn request_parts() -> axum::http::request::Parts {
        Request::new(()).into_parts().0
    }

    #[test]
    fn bundled_desktop_origin_is_strictly_loopback_http_with_dynamic_port() {
        assert!(is_bundled_desktop_origin(&HeaderValue::from_static(
            "http://127.0.0.1:43123"
        )));
        for denied in [
            "https://127.0.0.1:43123",
            "http://localhost:43123",
            "http://127.0.0.1",
            "http://127.0.0.2:43123",
            "https://example.com",
        ] {
            assert!(!is_bundled_desktop_origin(
                &HeaderValue::from_str(denied).expect("valid header")
            ));
        }
    }

    #[test]
    fn desktop_surface_is_recognized_on_actual_and_preflight_requests() {
        for (header, requested_header) in [
            (CHATOS_CLIENT_SURFACE_HEADER, "X-Chatos-Client-Surface"),
            (CHATOS_CLIENT_SURFACE_COMPAT_HEADER, "X-Requested-With"),
        ] {
            let mut actual = request_parts();
            actual.headers.insert(
                header,
                HeaderValue::from_static(LOCAL_CONNECTOR_DESKTOP_SURFACE),
            );
            assert!(request_has_desktop_surface(&actual));

            let mut preflight = request_parts();
            preflight.headers.insert(
                axum::http::header::ACCESS_CONTROL_REQUEST_HEADERS,
                HeaderValue::from_str(&format!("authorization, {requested_header}"))
                    .expect("valid preflight headers"),
            );
            assert!(request_has_desktop_surface(&preflight));
        }
        assert!(!request_has_desktop_surface(&request_parts()));
    }

    #[test]
    fn desktop_header_is_part_of_the_cors_allowlist() {
        for expected in [
            CHATOS_CLIENT_SURFACE_HEADER,
            CHATOS_CLIENT_SURFACE_COMPAT_HEADER,
        ] {
            assert!(allowed_headers()
                .iter()
                .any(|header| header.as_str() == expected));
        }
    }

    #[test]
    fn configured_origin_policy_is_not_replaced_by_loopback_support() {
        let configured = HeaderValue::from_static("https://app.example.com");
        let unrelated = HeaderValue::from_static("https://other.example.com");
        let loopback = HeaderValue::from_static("http://127.0.0.1:43123");
        let plain_request = request_parts();
        let configured_origins = [configured.clone()];

        assert!(origin_is_allowed(
            &configured_origins,
            &configured,
            &plain_request
        ));
        assert!(!origin_is_allowed(
            &configured_origins,
            &unrelated,
            &plain_request
        ));
        assert!(!origin_is_allowed(
            &configured_origins,
            &loopback,
            &plain_request
        ));

        for header in [
            CHATOS_CLIENT_SURFACE_HEADER,
            CHATOS_CLIENT_SURFACE_COMPAT_HEADER,
        ] {
            let mut desktop_request = request_parts();
            desktop_request.headers.insert(
                header,
                HeaderValue::from_static(LOCAL_CONNECTOR_DESKTOP_SURFACE),
            );
            assert!(origin_is_allowed(
                &configured_origins,
                &loopback,
                &desktop_request
            ));
            assert!(!origin_is_allowed(
                &configured_origins,
                &unrelated,
                &desktop_request
            ));
        }
    }
}
