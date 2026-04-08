# IM Service

IM Service is the messaging delivery layer of Agent Stack.

It is responsible for user/contact messaging, asynchronous reply delivery, WebSocket-based push, and IM-facing identity/auth coordination that supports the workspace experience.

IM Service 是 Agent Stack 的消息投递层。

它负责用户与联系人之间的消息收发、异步回复回推、基于 WebSocket 的主动推送，以及服务于工作空间体验的 IM 侧身份与鉴权协同。

## What This Service Does

- Stores IM-oriented message streams
- Pushes updates to connected clients over WebSocket
- Coordinates user/contact-facing delivery semantics
- Bridges asynchronous backend processing results back to the user layer

## Why It Exists

The IM experience should not be tightly coupled to raw model streaming or direct backend execution.

This service exists so the system can:
- treat chat as messaging, not just response streaming
- persist delivery-facing conversation state
- support asynchronous reply push after backend work completes
- separate user delivery concerns from orchestration concerns

## Structure

- `backend/`: Rust IM service

## Backend Quick Start

```bash
cd backend
cargo run --bin im_service
```

## More Docs

- [中文说明](./README.zh-CN.md)
- [English README](./README.en.md)
