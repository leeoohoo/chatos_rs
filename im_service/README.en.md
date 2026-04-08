# IM Service

## Overview

IM Service is the messaging delivery layer of Agent Stack.

It manages user/contact message persistence, asynchronous message push, and WebSocket-based delivery so the platform can provide a messaging-first experience instead of exposing raw backend execution directly.

## What It Is Responsible For

- IM-oriented message storage
- WebSocket push to online clients
- delivery semantics for user/contact interaction
- receiving backend completion results and pushing them to the user-facing layer

## Structure

- `backend/`: Rust service

## Backend Quick Start

```bash
cd backend
cargo run --bin im_service
```
