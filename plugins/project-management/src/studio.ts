import { PlanningStore } from './store.js';
import { startPlanningServer } from './http.js';

const port = Number(process.env.CHATOS_PLUGIN_APP_PORT);
if (!Number.isInteger(port) || port < 1 || port > 65535 || process.env.CHATOS_PLUGIN_APP_HOST !== '127.0.0.1') throw new Error('Host-provided loopback application address is required');
const store = new PlanningStore();
const { server } = await startPlanningServer(store, port);
for (const signal of ['SIGTERM', 'SIGINT'] as const) process.on(signal, () => server.close(() => { store.close(); process.exit(0); }));
