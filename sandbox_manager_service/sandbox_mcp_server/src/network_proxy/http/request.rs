// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::super::address::{normalize_host, parse_authority};
use super::super::*;

pub(in crate::network_proxy) async fn read_http_header<S>(stream: &mut S) -> Result<Vec<u8>, String>
where
    S: AsyncRead + Unpin,
{
    let mut bytes = Vec::with_capacity(2048);
    let mut buffer = [0_u8; 2048];
    loop {
        let read = stream
            .read(&mut buffer)
            .await
            .map_err(|err| err.to_string())?;
        if read == 0 {
            return Err("HTTP proxy client closed before sending headers".to_string());
        }
        bytes.extend_from_slice(&buffer[..read]);
        if bytes.len() > MAX_HTTP_HEADER_BYTES {
            return Err("HTTP proxy request headers exceed 64 KiB".to_string());
        }
        if bytes.windows(4).any(|window| window == b"\r\n\r\n") {
            return Ok(bytes);
        }
    }
}

#[derive(Debug)]
pub(in crate::network_proxy) struct ParsedHttpRequest {
    pub(in crate::network_proxy) method: String,
    pub(in crate::network_proxy) target: String,
    pub(in crate::network_proxy) version: u8,
    pub(in crate::network_proxy) headers: Vec<(String, String)>,
    pub(in crate::network_proxy) header_len: usize,
    pub(in crate::network_proxy) content_length: Option<u64>,
    pub(in crate::network_proxy) chunked: bool,
}

impl ParsedHttpRequest {
    pub(in crate::network_proxy) fn parse(bytes: &[u8]) -> Result<Self, String> {
        let mut headers = [httparse::EMPTY_HEADER; 128];
        let mut request = httparse::Request::new(&mut headers);
        let header_len = match request.parse(bytes).map_err(|err| err.to_string())? {
            httparse::Status::Complete(length) => length,
            httparse::Status::Partial => return Err("incomplete HTTP proxy request".to_string()),
        };
        let method = request
            .method
            .ok_or_else(|| "HTTP proxy request is missing a method".to_string())?
            .to_ascii_uppercase();
        let target = request
            .path
            .ok_or_else(|| "HTTP proxy request is missing a target".to_string())?
            .to_string();
        let version = request.version.unwrap_or(1);
        let headers = request
            .headers
            .iter()
            .map(|header| {
                let value = std::str::from_utf8(header.value)
                    .map_err(|_| "HTTP proxy header is not valid UTF-8".to_string())?;
                Ok((header.name.to_string(), value.trim().to_string()))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let content_length = unique_header_value(headers.as_slice(), "content-length")?
            .map(str::parse::<u64>)
            .transpose()
            .map_err(|_| "invalid Content-Length header".to_string())?;
        let transfer_encoding = unique_header_value(headers.as_slice(), "transfer-encoding")?;
        let chunked = match transfer_encoding {
            None => false,
            Some(value) if value.trim().eq_ignore_ascii_case("chunked") => true,
            Some(_) => {
                return Err(
                    "HTTP proxy only supports an exact Transfer-Encoding: chunked value"
                        .to_string(),
                )
            }
        };
        let _ = unique_header_value(headers.as_slice(), "host")?;
        let _ = unique_header_value(headers.as_slice(), "x-unix-socket")?;
        if chunked && content_length.is_some() {
            return Err("HTTP proxy rejects ambiguous body framing".to_string());
        }
        Ok(Self {
            method,
            target,
            version,
            headers,
            header_len,
            content_length,
            chunked,
        })
    }

    pub(in crate::network_proxy) fn destination(&self) -> Result<HttpDestination, String> {
        if let Ok(url) = Url::parse(self.target.as_str()) {
            if url.scheme() != "http" {
                return Err("non-CONNECT proxy requests must use http:// URLs".to_string());
            }
            let host = url
                .host_str()
                .ok_or_else(|| "HTTP proxy URL is missing a host".to_string())?;
            let port = url.port_or_known_default().unwrap_or(80);
            if let Some(host_header) = header_value(self.headers.as_slice(), "host") {
                let (header_host, header_port) = parse_authority(host_header, port)?;
                if normalize_host(header_host.as_str())? != normalize_host(host)?
                    || header_port != port
                {
                    return Err("HTTP proxy URL and Host header disagree".to_string());
                }
            }
            let mut origin_form = url.path().to_string();
            if origin_form.is_empty() {
                origin_form.push('/');
            }
            if let Some(query) = url.query() {
                origin_form.push('?');
                origin_form.push_str(query);
            }
            return Ok(HttpDestination {
                host: host.to_string(),
                port,
                origin_form,
            });
        }

        let host_header = header_value(self.headers.as_slice(), "host")
            .ok_or_else(|| "origin-form HTTP proxy request is missing Host".to_string())?;
        let (host, port) = parse_authority(host_header, 80)?;
        Ok(HttpDestination {
            host,
            port,
            origin_form: self.target.clone(),
        })
    }

    pub(in crate::network_proxy) fn https_origin_form(
        &self,
        expected_host: &str,
        expected_port: u16,
    ) -> Result<String, String> {
        let expected_host = normalize_host(expected_host)?;
        let host_header = header_value(self.headers.as_slice(), "host")
            .ok_or_else(|| "HTTPS request inside CONNECT is missing Host".to_string())?;
        let (header_host, header_port) = parse_authority(host_header, expected_port)?;
        if normalize_host(header_host.as_str())? != expected_host || header_port != expected_port {
            return Err("HTTPS Host header disagrees with the CONNECT target".to_string());
        }

        if let Ok(url) = Url::parse(self.target.as_str()) {
            if url.scheme() != "https" {
                return Err("absolute HTTPS proxy request must use an https:// URL".to_string());
            }
            let host = url
                .host_str()
                .ok_or_else(|| "absolute HTTPS proxy URL is missing a host".to_string())?;
            let port = url.port_or_known_default().unwrap_or(443);
            if normalize_host(host)? != expected_host || port != expected_port {
                return Err("absolute HTTPS proxy URL disagrees with CONNECT target".to_string());
            }
            let mut origin_form = url.path().to_string();
            if origin_form.is_empty() {
                origin_form.push('/');
            }
            if let Some(query) = url.query() {
                origin_form.push('?');
                origin_form.push_str(query);
            }
            return Ok(origin_form);
        }

        if self.target == "*" && self.method == "OPTIONS" {
            return Ok(self.target.clone());
        }
        if !self.target.starts_with('/') || self.target.starts_with("//") {
            return Err("HTTPS request target must use origin-form".to_string());
        }
        Ok(self.target.clone())
    }

    pub(in crate::network_proxy) fn forward_header(&self, origin_form: &str) -> String {
        let mut output = format!(
            "{} {} HTTP/1.{}\r\n",
            self.method, origin_form, self.version
        );
        let connection_headers = header_value(self.headers.as_slice(), "connection")
            .map(|value| {
                value
                    .split(',')
                    .map(|name| name.trim().to_ascii_lowercase())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for (name, value) in &self.headers {
            if is_hop_by_hop_header(name)
                || connection_headers
                    .iter()
                    .any(|candidate| name.eq_ignore_ascii_case(candidate))
            {
                continue;
            }
            output.push_str(name);
            output.push_str(": ");
            output.push_str(value);
            output.push_str("\r\n");
        }
        output.push_str("Connection: close\r\n\r\n");
        output
    }
}

#[derive(Debug)]
pub(in crate::network_proxy) struct HttpDestination {
    pub(in crate::network_proxy) host: String,
    pub(in crate::network_proxy) port: u16,
    pub(in crate::network_proxy) origin_form: String,
}

pub(in crate::network_proxy) fn header_value<'a>(
    headers: &'a [(String, String)],
    name: &str,
) -> Option<&'a str> {
    headers
        .iter()
        .find(|(header_name, _)| header_name.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

pub(in crate::network_proxy) fn unique_header_value<'a>(
    headers: &'a [(String, String)],
    name: &str,
) -> Result<Option<&'a str>, String> {
    let mut values = headers
        .iter()
        .filter(|(header_name, _)| header_name.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str());
    let first = values.next();
    if values.next().is_some() {
        return Err(format!("HTTP proxy rejects duplicate {name} headers"));
    }
    Ok(first)
}

pub(in crate::network_proxy) fn is_hop_by_hop_header(name: &str) -> bool {
    [
        "connection",
        "proxy-connection",
        "proxy-authorization",
        "proxy-authenticate",
        "keep-alive",
        "upgrade",
        "x-unix-socket",
    ]
    .iter()
    .any(|candidate| name.eq_ignore_ascii_case(candidate))
}
