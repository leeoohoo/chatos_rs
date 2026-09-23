#[cfg(test)]
mod tests {
    use async_tungstenite::{
        tokio::connect_async,
        tungstenite::{
            Message,
            client::IntoClientRequest,
            http::{HeaderValue, header::SEC_WEBSOCKET_PROTOCOL},
        },
    };
    use futures::StreamExt;
    use tempfile::tempdir;
    use tokio::sync::oneshot;

    use super::*;

    const EXTENSION_ID: &str = "nkeimhogjdpnpccoofpliimaahmaaome";

    #[tokio::test]
    async fn pairing_survives_bridge_restart_and_is_bound_to_extension_identity() {
        let directory = tempdir().unwrap();
        let config = BridgeServerConfig::development(directory.path().to_owned(), EXTENSION_ID);
        let (server, _) = BridgeServer::bind(config.clone()).await.unwrap();
        assert!(!server.state.paired.load(Ordering::Acquire));
        persist_pairing(&server.state).await.unwrap();
        drop(server);

        let (restarted, _) = BridgeServer::bind(config).await.unwrap();
        assert!(restarted.state.paired.load(Ordering::Acquire));
        drop(restarted);

        let (different_extension, _) = BridgeServer::bind(BridgeServerConfig::development(
            directory.path().to_owned(),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ))
        .await
        .unwrap();
        assert!(!different_extension.state.paired.load(Ordering::Acquire));
    }

    #[tokio::test]
    async fn relays_browser_commands_and_extension_events() {
        let directory = tempdir().unwrap();
        let (server, ready) = BridgeServer::bind(BridgeServerConfig::development(
            directory.path().to_owned(),
            EXTENSION_ID,
        ))
        .await
        .unwrap();
        let persisted: BridgeStateFile =
            serde_json::from_slice(&tokio::fs::read(&ready.state_file).await.unwrap()).unwrap();
        let credential: Value =
            serde_json::from_slice(&tokio::fs::read(&ready.mcp_credential_file).await.unwrap())
                .unwrap();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let server_task = tokio::spawn(server.serve_until(async {
            let _ = shutdown_rx.await;
        }));

        let mut control = connect(&persisted.control_endpoint, CONTROL_SUBPROTOCOL).await;
        send_json(
            &mut control,
            json!({
                "type":"request","id":1,"method":"control.authenticate",
                "params":{"protocol_version":PROTOCOL_VERSION,"token":persisted.control_token}
            }),
        )
        .await;
        assert_eq!(receive_json(&mut control).await["id"], 1);
        send_json(
            &mut control,
            json!({
                "type":"request","id":2,"method":"control.bootstrapExtension",
                "params":{"origin":format!("chrome-extension://{EXTENSION_ID}/"),"pairing_requested":true}
            }),
        )
        .await;
        let bootstrap = receive_json(&mut control).await;
        let extension_token = bootstrap["result"]["token"].as_str().unwrap().to_owned();

        let mut extension = connect(&persisted.extension_endpoint, EXTENSION_SUBPROTOCOL).await;
        send_json(
            &mut extension,
            json!({
                "type":"request","id":1,"method":"extension.authenticate",
                "params":{"protocol_version":PROTOCOL_VERSION,"token":extension_token}
            }),
        )
        .await;
        assert_eq!(
            receive_json(&mut extension).await["result"]["protocol_version"],
            "1.0"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;

        let mut browser = connect(&persisted.browser_endpoint, BROWSER_SUBPROTOCOL).await;
        send_json(
            &mut browser,
            json!({
                "type":"request","id":1,"method":"bridge.authenticate",
                "params":{"protocol_version":PROTOCOL_VERSION,"token":credential["token"]}
            }),
        )
        .await;
        let capabilities_request = receive_json(&mut extension).await;
        assert_eq!(capabilities_request["method"], "extension.getCapabilities");
        send_json(
            &mut extension,
            json!({
                "type":"response","id":capabilities_request["id"],
                "result":{"capabilities":["page_control","raw_cdp","native_tab_groups"]}
            }),
        )
        .await;
        assert_eq!(
            receive_json(&mut browser).await["result"]["protocol_version"],
            "1.0"
        );

        send_json(
            &mut browser,
            json!({"type":"request","id":2,"method":"bridge.listTargets","params":{}}),
        )
        .await;
        let extension_request = receive_json(&mut extension).await;
        assert_eq!(extension_request["method"], "extension.listTargets");
        send_json(
            &mut extension,
            json!({
                "type":"response","id":extension_request["id"],
                "result":{"targets":[{"id":"target_one","title":"Shared","url":"https://example.test/","kind":"page"}]}
            }),
        )
        .await;
        let targets = receive_json(&mut browser).await;
        assert_eq!(targets["result"]["targets"][0]["id"], "target_one");

        send_json(
            &mut browser,
            json!({
                "type":"request","id":3,"method":"bridge.subscribe",
                "params":{"subscription_id":"sub_one","session_id":"session_one","methods":["Runtime.consoleAPICalled"]}
            }),
        )
        .await;
        let subscribe = receive_json(&mut extension).await;
        assert_eq!(subscribe["method"], "extension.subscribe");
        send_json(
            &mut extension,
            json!({"type":"response","id":subscribe["id"],"result":{}}),
        )
        .await;
        assert_eq!(receive_json(&mut browser).await["id"], 3);
        send_json(
            &mut extension,
            json!({
                "type":"event","method":"extension.cdpEvent",
                "params":{"subscription_id":"sub_one","session_id":"session_one","method":"Runtime.consoleAPICalled","params":{"type":"log"}}
            }),
        )
        .await;
        let event = receive_json(&mut browser).await;
        assert_eq!(event["method"], "cdp.event");
        assert_eq!(event["params"]["subscription_id"], "sub_one");

        send_json(
            &mut browser,
            json!({"type":"request","id":4,"method":"bridge.close","params":{}}),
        )
        .await;
        let end_session = receive_json(&mut extension).await;
        assert_eq!(end_session["method"], "extension.endSession");
        send_json(
            &mut extension,
            json!({"type":"response","id":end_session["id"],"result":{}}),
        )
        .await;
        assert_eq!(receive_json(&mut browser).await["id"], 4);

        // The credential remains private and time-bounded, but it can be
        // reused after the previous authenticated MCP connection closes. This
        // lets a task recover from a transient extension/Bridge disconnect.
        tokio::time::sleep(Duration::from_millis(20)).await;
        let mut reconnected_browser =
            connect(&persisted.browser_endpoint, BROWSER_SUBPROTOCOL).await;
        send_json(
            &mut reconnected_browser,
            json!({
                "type":"request","id":1,"method":"bridge.authenticate",
                "params":{"protocol_version":PROTOCOL_VERSION,"token":credential["token"]}
            }),
        )
        .await;
        let capabilities_request = receive_json(&mut extension).await;
        assert_eq!(capabilities_request["method"], "extension.getCapabilities");
        send_json(
            &mut extension,
            json!({
                "type":"response","id":capabilities_request["id"],
                "result":{"capabilities":["page_control","raw_cdp","native_tab_groups"]}
            }),
        )
        .await;
        assert_eq!(
            receive_json(&mut reconnected_browser).await["result"]["protocol_version"],
            "1.0"
        );
        let _ = shutdown_tx.send(());
        server_task.await.unwrap().unwrap();
    }

    async fn connect(
        endpoint: &str,
        subprotocol: &'static str,
    ) -> async_tungstenite::WebSocketStream<async_tungstenite::tokio::ConnectStream> {
        let mut request = endpoint.into_client_request().unwrap();
        request.headers_mut().insert(
            SEC_WEBSOCKET_PROTOCOL,
            HeaderValue::from_static(subprotocol),
        );
        let (socket, response) = connect_async(request).await.unwrap();
        assert_eq!(
            response
                .headers()
                .get(SEC_WEBSOCKET_PROTOCOL)
                .unwrap()
                .to_str()
                .unwrap(),
            subprotocol
        );
        socket
    }

    async fn send_json<S>(socket: &mut WebSocketStream<S>, value: Value)
    where
        S: futures::AsyncRead + futures::AsyncWrite + Unpin,
    {
        socket.send(Message::text(value.to_string())).await.unwrap();
    }

    async fn receive_json<S>(socket: &mut WebSocketStream<S>) -> Value
    where
        S: futures::AsyncRead + futures::AsyncWrite + Unpin,
    {
        loop {
            match socket.next().await.unwrap().unwrap() {
                Message::Text(text) => return serde_json::from_str(&text).unwrap(),
                Message::Ping(_) | Message::Pong(_) => continue,
                message => panic!("unexpected WebSocket message: {message:?}"),
            }
        }
    }
}
