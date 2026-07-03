# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

FROM node:20-bookworm-slim AS frontend-builder
WORKDIR /app/official_website_service/frontend

COPY official_website_service/frontend/package*.json ./
RUN npm ci

COPY official_website_service/frontend/ ./
RUN npm run build

FROM rust:1.85-bookworm AS backend-builder
WORKDIR /app

COPY . .
COPY --from=frontend-builder /app/official_website_service/frontend/dist ./official_website_service/frontend/dist
RUN cargo build --release -p official_website_service_backend

FROM debian:bookworm-slim AS runtime
WORKDIR /app

RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates curl \
  && rm -rf /var/lib/apt/lists/*

COPY --from=backend-builder /app/target/release/official_website_service_backend /usr/local/bin/official_website_service_backend
COPY --from=frontend-builder /app/official_website_service/frontend/dist ./frontend/dist

ENV OFFICIAL_WEBSITE_HOST=0.0.0.0 \
    OFFICIAL_WEBSITE_PORT=39250 \
    OFFICIAL_WEBSITE_MODE=prod \
    OFFICIAL_WEBSITE_PUBLIC_BASE_URL=http://localhost:39250 \
    OFFICIAL_WEBSITE_ENABLE_LIVE_STATUS=false \
    OFFICIAL_WEBSITE_STATIC_DIR=/app/frontend/dist

EXPOSE 39250
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD curl -fsS http://127.0.0.1:${OFFICIAL_WEBSITE_PORT}/health || exit 1

CMD ["official_website_service_backend"]
