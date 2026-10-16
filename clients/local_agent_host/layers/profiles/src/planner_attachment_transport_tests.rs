use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

struct LoopbackModel {
    endpoint: String,
    responses: bool,
}

#[async_trait]
impl LocalModelRuntimeResolver for LoopbackModel {
    async fn resolve_model_runtime(
        &self,
        _: &str,
        _: &str,
        _: &str,
    ) -> Result<TransientLocalModelRuntime, String> {
        Ok(TransientLocalModelRuntime {
            runner: Arc::new(ContextualTurnRunner::new(AiRuntime::new(None), None)),
            model_config: ModelRuntimeConfig::openai_compatible(
                &self.endpoint,
                "synthetic-key",
                "test-image-model",
                "gpt",
            )
            .with_responses_support(self.responses)
            .with_images_support(Some(true))
            .with_max_transient_retries(Some(0)),
        })
    }
}

// This is a transport contract test, not a vision-quality evaluation. No live
// provider, credentials, private image, or business Task is used.
#[tokio::test]
async fn attachment_pixels_reach_actual_http_for_both_profiles_and_legacy_protocol_flags() {
    for responses in [false, true] {
        for task in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let endpoint = format!("http://{}", listener.local_addr().unwrap());
            listener.set_nonblocking(true).unwrap();
            let server = std::thread::spawn(move || {
                let started = Instant::now();
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(
                                started.elapsed() < Duration::from_secs(10),
                                "model request never reached loopback"
                            );
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        Err(error) => panic!("loopback accept: {error}"),
                    }
                };
                // Darwin can inherit O_NONBLOCK from the listening socket.
                // A successful accept does not mean the request bytes arrived yet.
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = Vec::new();
                let header_end = loop {
                    let mut buffer = [0u8; 8192];
                    let count = stream.read(&mut buffer).unwrap();
                    assert!(count > 0, "missing HTTP request body");
                    bytes.extend_from_slice(&buffer[..count]);
                    if let Some(index) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                        break index + 4;
                    }
                };
                let headers = String::from_utf8_lossy(&bytes[..header_end]);
                let size = headers
                    .lines()
                    .find_map(|line| {
                        let (key, value) = line.split_once(':')?;
                        key.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().unwrap())
                    })
                    .unwrap();
                while bytes.len() < header_end + size {
                    let mut buffer = [0u8; 8192];
                    let count = stream.read(&mut buffer).unwrap();
                    assert!(count > 0);
                    bytes.extend_from_slice(&buffer[..count]);
                }
                let payload: Value =
                    serde_json::from_slice(&bytes[header_end..header_end + size]).unwrap();
                // Responses is the unified protocol even for a legacy false flag.
                let body = format!(
                    "data: {}\n\n",
                    json!({"type":"response.completed","response":{
                        "id":"synthetic-image-response","status":"completed","output_text":"已完成图片输入验证", "output":[]
                    }})
                );
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
                payload
            });
            let resolver = LoopbackModel {
                endpoint,
                responses,
            };
            let planner = if task {
                ControlPlaneLocalAiStepPlanner::task_execution(
                    resolver,
                    OwnerCheckingCapabilityResolver,
                )
            } else {
                ControlPlaneLocalAiStepPlanner::main_chat(resolver, OwnerCheckingCapabilityResolver)
            };
            let prepared = planner.prepare_ai_step(&image_claim(task)).await.unwrap();
            prepared
                .runner
                .execute_once(prepared.request, 2, prepared.reason, 1)
                .await
                .unwrap();
            let payload = server.join().unwrap();
            let messages = payload["input"].as_array().unwrap_or_else(|| {
                panic!(
                    "missing message array (responses={responses}); payload keys: {:?}",
                    payload.as_object().unwrap().keys().collect::<Vec<_>>()
                )
            });
            assert!(
                messages
                    .iter()
                    .filter_map(|item| item["content"].as_array())
                    .flatten()
                    .any(|part| {
                        part["type"] == "input_image"
                            && part["image_url"] == "data:image/png;base64,iVBORw0KGgo="
                    }),
                "attachment must be a real image part in the HTTP body"
            );
        }
    }
}
