# Official Website Service

The official website service is part of the Docker cloud stack.

## Run

From the repository root:

```bash
docker/deploy.sh up
```

Default URLs:

- Frontend: `http://localhost:39251`
- Backend: `http://localhost:39250`

## Build Locally

```bash
cd official_website_service/frontend
npm install
npm run build
```

```bash
cargo build -p official_website_service_backend
```

## Configuration

Docker deployment uses `docker/.env` and `docker/compose.yml`.

The backend exposes:

- `GET /health`
- `GET /api/site/status`

The frontend container proxies API requests to `official-website-backend` inside the Compose network.
