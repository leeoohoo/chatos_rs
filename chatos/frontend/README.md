# chatos/frontend

Main React frontend for Chatos RS.

## Docker Stack

From the repository root:

```bash
docker/deploy.sh up
```

Default URL: `http://localhost:8088`

## Frontend-Only Development

```bash
npm install
npm run dev
```

## Checks

```bash
npm run build
npm run type-check
npm run test -- --run
npm run lint
```
