from __future__ import annotations

from gateway_base.logging import state_log
from gateway_core.runtime import parse_args
from gateway_http.handler import GatewayServer
from gateway_runtime.sdk_types import SDK_IMPORT_SOURCE


def main() -> None:
    cfg = parse_args()
    server = GatewayServer((cfg.host, cfg.port), cfg)
    print(f"OpenAI-compatible gateway listening on http://{cfg.host}:{cfg.port}")
    state_log("sdk", f"source={SDK_IMPORT_SOURCE}")
    state_log("startup", f"state_db={cfg.state_db_path}")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
