# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

from pathlib import Path
import re
import unittest


ROOT = Path(__file__).resolve().parents[2]
PRODUCTION_BACKENDS = (
    "configuration-center-backend",
    "harness",
    "user-service-backend",
    "memory-engine-backend",
    "plugin-management-backend",
    "local-connector-service-backend",
    "official-website-backend",
)


def service_block(compose: str, service_name: str) -> str:
    match = re.search(
        rf"^  {re.escape(service_name)}:\n(?P<body>.*?)(?=^  [a-zA-Z0-9_-]+:\n|\Z)",
        compose,
        flags=re.MULTILINE | re.DOTALL,
    )
    if match is None:
        raise AssertionError(f"missing Compose service: {service_name}")
    return match.group("body")


class ContainerNetworkBoundaryTests(unittest.TestCase):
    def test_production_backends_are_internal_only(self) -> None:
        compose = (ROOT / "docker/compose.yml").read_text()
        for service_name in PRODUCTION_BACKENDS:
            with self.subTest(service=service_name):
                self.assertNotIn("\n    ports:", service_block(compose, service_name))
        self.assertNotIn("CHATOS_PUBLIC_BIND_HOST", compose)

    def test_gateway_is_published_only_on_loopback_by_default(self) -> None:
        platform = (ROOT / "docker/compose.platform.yml").read_text()
        gateway = service_block(platform, "apisix-gateway")
        self.assertIn(
            '"${CHATOS_GATEWAY_BIND_HOST:-127.0.0.1}:${APISIX_GATEWAY_PORT:-9080}:9080"',
            gateway,
        )
        self.assertNotIn("CHATOS_PUBLIC_BIND_HOST", gateway)

        bootstrap = (ROOT / "docker/bootstrap.conf.example").read_text()
        self.assertIn("CHATOS_GATEWAY_BIND_HOST=127.0.0.1", bootstrap)
        self.assertNotIn("CHATOS_PUBLIC_BIND_HOST", bootstrap)

    def test_local_dev_harness_ports_are_loopback_only(self) -> None:
        local_dev = (ROOT / "docker/compose.local-dev.yml").read_text()
        harness = service_block(local_dev, "harness")
        self.assertIn("${CHATOS_INFRA_BIND_HOST:-127.0.0.1}", harness)
        self.assertNotIn("0.0.0.0", harness)

    def test_deploy_output_does_not_advertise_internal_services(self) -> None:
        deploy = (ROOT / "docker/deploy.sh").read_text()
        self.assertNotIn("Local Connector Service:  http://localhost", deploy)
        self.assertNotIn("Harness:                  http://localhost", deploy)


if __name__ == "__main__":
    unittest.main()
