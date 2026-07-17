// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, IsCa, Issuer, KeyPair,
    KeyUsagePurpose, PKCS_ECDSA_P256_SHA256,
};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName};
use rustls::{ClientConfig, RootCertStore, ServerConfig};
use std::collections::BTreeMap;
use std::io::BufReader as StdBufReader;
use std::sync::Arc;
#[cfg(unix)]
use tokio::net::UnixListener;
use tokio_rustls::{TlsAcceptor, TlsConnector};

fn requirements(entries: &[(&str, NetworkDomainPermission)]) -> NetworkRequirements {
    NetworkRequirements {
        enabled: Some(true),
        domains: Some(
            entries
                .iter()
                .map(|(host, permission)| ((*host).to_string(), *permission))
                .collect(),
        ),
        ..Default::default()
    }
}

fn test_upstream_tls_acceptor() -> (TlsAcceptor, CertificateDer<'static>) {
    let mut ca_params = CertificateParams::default();
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::DigitalSignature,
    ];
    let mut distinguished_name = DistinguishedName::new();
    distinguished_name.push(DnType::CommonName, "ChatOS network proxy test CA");
    ca_params.distinguished_name = distinguished_name;
    let ca_key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).expect("test CA key");
    let ca_certificate = ca_params.self_signed(&ca_key).expect("test CA certificate");
    let ca_der = ca_certificate.der().clone();
    let issuer = Issuer::new(ca_params, ca_key);

    let leaf_params = CertificateParams::new(vec!["localhost".to_string()]).expect("leaf params");
    let leaf_key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).expect("leaf key");
    let leaf_certificate = leaf_params
        .signed_by(&leaf_key, &issuer)
        .expect("leaf certificate");
    let private_key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(leaf_key.serialize_der()));
    let mut server_config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![leaf_certificate.der().clone()], private_key)
        .expect("TLS server config");
    server_config.alpn_protocols = vec![b"http/1.1".to_vec()];
    (TlsAcceptor::from(Arc::new(server_config)), ca_der)
}

fn client_tls_connector(bundle_path: &Path) -> TlsConnector {
    let file = std::fs::File::open(bundle_path).expect("open managed trust bundle");
    let mut reader = StdBufReader::new(file);
    let certificates = rustls_pemfile::certs(&mut reader)
        .collect::<Result<Vec<_>, _>>()
        .expect("parse managed trust bundle");
    let mut roots = RootCertStore::empty();
    let (added, _ignored) = roots.add_parsable_certificates(certificates);
    assert!(added > 0);
    let mut config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    TlsConnector::from(Arc::new(config))
}

#[test]
fn wildcard_semantics_and_deny_precedence_match_codex_profiles() {
    let policy = NetworkProxyPolicy::from_requirements(&requirements(&[
        ("**.example.com", NetworkDomainPermission::Allow),
        ("private.example.com", NetworkDomainPermission::Deny),
    ]))
    .expect("policy");
    assert!(policy
        .allow
        .iter()
        .any(|pattern| pattern.matches("example.com")));
    assert!(policy
        .allow
        .iter()
        .any(|pattern| pattern.matches("api.example.com")));
    assert!(policy
        .deny
        .iter()
        .any(|pattern| pattern.matches("private.example.com")));

    let subdomains = DomainPattern::parse("*.example.com").expect("subdomains");
    assert!(!subdomains.matches("example.com"));
    assert!(subdomains.matches("api.example.com"));
}

#[test]
fn private_and_special_ip_ranges_are_classified_as_non_public() {
    for ip in [
        "127.0.0.1",
        "10.0.0.1",
        "100.64.0.1",
        "169.254.1.1",
        "192.0.2.1",
        "198.18.0.1",
        "224.0.0.1",
        "::1",
        "fe80::1",
        "fc00::1",
    ] {
        assert!(is_non_public_ip(ip.parse().expect("IP")), "{ip}");
    }
    assert!(!is_non_public_ip("1.1.1.1".parse().expect("IP")));
    assert!(!is_non_public_ip(
        "2606:4700:4700::1111".parse().expect("IP")
    ));
}

#[tokio::test]
async fn explicit_literal_allow_is_required_for_loopback() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("loopback listener");
    let port = listener.local_addr().expect("address").port();

    let wildcard = NetworkProxyPolicy::from_requirements(&requirements(&[(
        "*",
        NetworkDomainPermission::Allow,
    )]))
    .expect("wildcard policy");
    assert!(wildcard.connect("127.0.0.1", port).await.is_err());

    let exact = NetworkProxyPolicy::from_requirements(&requirements(&[(
        "127.0.0.1",
        NetworkDomainPermission::Allow,
    )]))
    .expect("exact policy");
    assert!(exact.connect("127.0.0.1", port).await.is_ok());
}

#[tokio::test]
async fn http_proxy_enforces_allowlist_and_forwards_allowed_request() {
    let upstream = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("upstream");
    let upstream_port = upstream.local_addr().expect("upstream address").port();
    tokio::spawn(async move {
        let (mut stream, _) = upstream.accept().await.expect("accept upstream");
        let mut request = vec![0_u8; 4096];
        let _ = stream.read(&mut request).await.expect("read request");
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
            .await
            .expect("write response");
    });

    let root = std::env::temp_dir().join(format!(
        "chatos-network-proxy-test-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).expect("state root");
    let runtime = NetworkProxyRuntime::start(
        root.as_path(),
        &NetworkRequirements {
            enabled: Some(true),
            domains: Some(BTreeMap::from([(
                "127.0.0.1".to_string(),
                NetworkDomainPermission::Allow,
            )])),
            enable_socks5: Some(false),
            ..Default::default()
        },
    )
    .await
    .expect("start proxy")
    .expect("enabled proxy");

    let mut client = TcpStream::connect((Ipv4Addr::LOCALHOST, runtime.endpoints.http_port))
        .await
        .expect("connect proxy");
    client
        .write_all(
            format!(
                "GET http://127.0.0.1:{upstream_port}/health HTTP/1.1\r\nHost: 127.0.0.1:{upstream_port}\r\n\r\n"
            )
            .as_bytes(),
        )
        .await
        .expect("write proxy request");
    let mut response = String::new();
    client
        .read_to_string(&mut response)
        .await
        .expect("read proxy response");
    assert!(response.contains("200 OK"), "{response}");
    assert!(response.ends_with("ok"), "{response}");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn https_inner_request_rejects_host_mismatch_and_ambiguous_targets() {
    let mismatched = ParsedHttpRequest::parse(b"GET / HTTP/1.1\r\nHost: other.example.com\r\n\r\n")
        .expect("request");
    assert!(mismatched.https_origin_form("example.com", 443).is_err());

    let network_path = ParsedHttpRequest::parse(
        b"GET //other.example.com/path HTTP/1.1\r\nHost: example.com\r\n\r\n",
    )
    .expect("request");
    assert!(network_path.https_origin_form("example.com", 443).is_err());
}

#[tokio::test]
async fn limited_https_connect_mitm_allows_get() {
    let (upstream_acceptor, upstream_ca) = test_upstream_tls_acceptor();
    let upstream_listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("upstream listener");
    let upstream_port = upstream_listener.local_addr().expect("address").port();
    let upstream_task = tokio::spawn(async move {
        let (stream, _) = upstream_listener.accept().await.expect("accept upstream");
        let mut stream = upstream_acceptor
            .accept(stream)
            .await
            .expect("upstream TLS");
        let header = read_http_header(&mut stream)
            .await
            .expect("upstream request");
        let request = ParsedHttpRequest::parse(header.as_slice()).expect("parse request");
        assert_eq!(request.method, "GET");
        assert_eq!(request.target, "/health");
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
            .await
            .expect("write upstream response");
        stream.shutdown().await.expect("shutdown upstream TLS");
    });

    let state = tempfile::tempdir().expect("state");
    let mitm = Arc::new(
        MitmAuthority::new_with_additional_roots(state.path(), vec![upstream_ca])
            .expect("MITM authority"),
    );
    let policy = Arc::new(
        NetworkProxyPolicy::from_requirements(&NetworkRequirements {
            enabled: Some(true),
            mode: Some(NetworkProxyMode::Limited),
            domains: Some(BTreeMap::from([(
                "localhost".to_string(),
                NetworkDomainPermission::Allow,
            )])),
            enable_socks5: Some(false),
            ..Default::default()
        })
        .expect("policy"),
    );
    let proxy_listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("proxy listener");
    let proxy_port = proxy_listener.local_addr().expect("address").port();
    let handler_mitm = mitm.clone();
    let proxy_task = tokio::spawn(async move {
        let (stream, _) = proxy_listener.accept().await.expect("accept proxy client");
        handle_http_connection(stream, policy, Some(handler_mitm)).await
    });

    let mut client = TcpStream::connect((Ipv4Addr::LOCALHOST, proxy_port))
        .await
        .expect("connect proxy");
    client
        .write_all(
            format!(
                "CONNECT localhost:{upstream_port} HTTP/1.1\r\nHost: localhost:{upstream_port}\r\n\r\n"
            )
            .as_bytes(),
        )
        .await
        .expect("write CONNECT");
    let response = read_http_header(&mut client)
        .await
        .expect("CONNECT response");
    assert!(String::from_utf8_lossy(response.as_slice()).contains("200 Connection Established"));

    let connector = client_tls_connector(mitm.ca_bundle_path());
    let server_name = ServerName::try_from("localhost".to_string()).expect("server name");
    let mut client = connector
        .connect(server_name, client)
        .await
        .expect("client TLS");
    client
        .write_all(
            format!(
                "GET /health HTTP/1.1\r\nHost: localhost:{upstream_port}\r\nConnection: close\r\n\r\n"
            )
            .as_bytes(),
        )
        .await
        .expect("write HTTPS request");
    let mut response = String::new();
    client
        .read_to_string(&mut response)
        .await
        .expect("read HTTPS response");
    assert!(response.contains("200 OK"), "{response}");
    assert!(response.ends_with("ok"), "{response}");
    proxy_task.await.expect("proxy task").expect("proxy result");
    upstream_task.await.expect("upstream task");
}

#[tokio::test]
async fn limited_https_connect_mitm_blocks_post_before_upstream_tls() {
    let upstream_listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("upstream listener");
    let upstream_port = upstream_listener.local_addr().expect("address").port();
    let upstream_task = tokio::spawn(async move {
        let (mut stream, _) = upstream_listener.accept().await.expect("accept upstream");
        let mut byte = [0_u8; 1];
        let read = stream.read(&mut byte).await.expect("read upstream");
        assert_eq!(read, 0, "method policy must run before upstream TLS");
    });

    let state = tempfile::tempdir().expect("state");
    let mitm = Arc::new(MitmAuthority::new(state.path()).expect("MITM authority"));
    let policy = Arc::new(
        NetworkProxyPolicy::from_requirements(&NetworkRequirements {
            enabled: Some(true),
            mode: Some(NetworkProxyMode::Limited),
            domains: Some(BTreeMap::from([(
                "127.0.0.1".to_string(),
                NetworkDomainPermission::Allow,
            )])),
            enable_socks5: Some(false),
            ..Default::default()
        })
        .expect("policy"),
    );
    let proxy_listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("proxy listener");
    let proxy_port = proxy_listener.local_addr().expect("address").port();
    let handler_mitm = mitm.clone();
    let proxy_task = tokio::spawn(async move {
        let (stream, _) = proxy_listener.accept().await.expect("accept proxy client");
        handle_http_connection(stream, policy, Some(handler_mitm)).await
    });

    let mut client = TcpStream::connect((Ipv4Addr::LOCALHOST, proxy_port))
        .await
        .expect("connect proxy");
    client
        .write_all(
            format!(
                "CONNECT 127.0.0.1:{upstream_port} HTTP/1.1\r\nHost: 127.0.0.1:{upstream_port}\r\n\r\n"
            )
            .as_bytes(),
        )
        .await
        .expect("write CONNECT");
    let response = read_http_header(&mut client)
        .await
        .expect("CONNECT response");
    assert!(String::from_utf8_lossy(response.as_slice()).contains("200 Connection Established"));

    let connector = client_tls_connector(mitm.ca_bundle_path());
    let server_name = ServerName::try_from("127.0.0.1".to_string()).expect("server name");
    let mut client = connector
        .connect(server_name, client)
        .await
        .expect("client TLS");
    client
        .write_all(
            format!(
                "POST /upload HTTP/1.1\r\nHost: 127.0.0.1:{upstream_port}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            )
            .as_bytes(),
        )
        .await
        .expect("write HTTPS request");
    let mut response = String::new();
    client
        .read_to_string(&mut response)
        .await
        .expect("read HTTPS response");
    assert!(response.contains("403 Forbidden"), "{response}");
    assert!(response.contains("blocked-by-method-policy"), "{response}");
    proxy_task.await.expect("proxy task").expect("proxy result");
    upstream_task.await.expect("upstream task");
}

#[tokio::test]
async fn limited_socks_rejects_non_https_ports() {
    let policy = Arc::new(
        NetworkProxyPolicy::from_requirements(&NetworkRequirements {
            enabled: Some(true),
            mode: Some(NetworkProxyMode::Limited),
            domains: Some(BTreeMap::from([(
                "127.0.0.1".to_string(),
                NetworkDomainPermission::Allow,
            )])),
            ..Default::default()
        })
        .expect("policy"),
    );
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("SOCKS listener");
    let port = listener.local_addr().expect("address").port();
    let handler = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept SOCKS client");
        handle_socks_connection(stream, policy, None).await
    });

    let mut client = TcpStream::connect((Ipv4Addr::LOCALHOST, port))
        .await
        .expect("connect SOCKS");
    client
        .write_all(&[5, 1, 0])
        .await
        .expect("write SOCKS methods");
    let mut method_reply = [0_u8; 2];
    client
        .read_exact(&mut method_reply)
        .await
        .expect("read SOCKS method reply");
    assert_eq!(method_reply, [5, 0]);

    client
        .write_all(&[5, 1, 0, 1, 127, 0, 0, 1, 0, 80])
        .await
        .expect("write SOCKS request");
    let mut reply = [0_u8; 10];
    client
        .read_exact(&mut reply)
        .await
        .expect("read SOCKS reply");
    assert_eq!(reply[1], 7);
    handler
        .await
        .expect("handler task")
        .expect("handler result");
}

#[tokio::test]
async fn limited_proxy_injects_and_cleans_up_managed_ca_bundle() {
    let state = tempfile::tempdir().expect("state");
    let runtime = NetworkProxyRuntime::start(
        state.path(),
        &NetworkRequirements {
            enabled: Some(true),
            mode: Some(NetworkProxyMode::Limited),
            domains: Some(BTreeMap::from([(
                "example.com".to_string(),
                NetworkDomainPermission::Allow,
            )])),
            enable_socks5: Some(false),
            ..Default::default()
        },
    )
    .await
    .expect("start proxy")
    .expect("enabled proxy");
    let bundle_path = runtime
        .endpoints
        .ca_bundle_path
        .clone()
        .expect("managed CA bundle");
    assert!(bundle_path.is_file());

    let mut command = tokio::process::Command::new("/usr/bin/true");
    runtime.endpoints.apply_to_command(&mut command);
    let environment = command
        .as_std()
        .get_envs()
        .filter_map(|(name, value)| {
            value.map(|value| {
                (
                    name.to_string_lossy().to_string(),
                    value.to_string_lossy().to_string(),
                )
            })
        })
        .collect::<BTreeMap<_, _>>();
    for name in CUSTOM_CA_ENV_KEYS {
        #[cfg(windows)]
        let value = environment
            .iter()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str());
        #[cfg(not(windows))]
        let value = environment.get(name).map(String::as_str);
        assert_eq!(
            value,
            Some(bundle_path.to_string_lossy().as_ref()),
            "{name}"
        );
    }

    drop(runtime);
    assert!(!bundle_path.exists());
}

#[cfg(unix)]
#[tokio::test]
async fn http_proxy_enforces_unix_socket_allowlist() {
    let root = std::env::temp_dir().join(format!(
        "chatos-network-unix-proxy-test-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).expect("state root");
    let socket_path = PathBuf::from(format!(
        "/tmp/chatos-unix-proxy-{}.sock",
        uuid::Uuid::new_v4()
    ));
    let listener = UnixListener::bind(socket_path.as_path()).expect("unix listener");
    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept unix request");
        let mut request = vec![0_u8; 4096];
        let _ = stream.read(&mut request).await.expect("read unix request");
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\nunix")
            .await
            .expect("write unix response");
    });

    let runtime = NetworkProxyRuntime::start(
        root.as_path(),
        &NetworkRequirements {
            enabled: Some(true),
            unix_sockets: Some(BTreeMap::from([(
                socket_path.to_string_lossy().to_string(),
                NetworkDomainPermission::Allow,
            )])),
            enable_socks5: Some(false),
            ..Default::default()
        },
    )
    .await
    .expect("start proxy")
    .expect("enabled proxy");

    let mut client = TcpStream::connect((Ipv4Addr::LOCALHOST, runtime.endpoints.http_port))
        .await
        .expect("connect proxy");
    client
        .write_all(
            format!(
                "GET http://unix.local/health HTTP/1.1\r\nHost: unix.local\r\nx-unix-socket: {}\r\n\r\n",
                socket_path.display()
            )
            .as_bytes(),
        )
        .await
        .expect("write proxy request");
    let mut response = String::new();
    client
        .read_to_string(&mut response)
        .await
        .expect("read proxy response");
    assert!(response.contains("200 OK"), "{response}");
    assert!(response.ends_with("unix"), "{response}");
    let _ = std::fs::remove_file(socket_path);
    let _ = std::fs::remove_dir_all(root);
}
