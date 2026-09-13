import pathlib
import subprocess
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
ENTRYPOINT = ROOT / "scripts" / "local-client-stack.sh"
COMMON_ENTRYPOINT = ROOT / "scripts" / "local-dev-stack.sh"
RUNNER = ROOT / "scripts" / "local-dev-stack" / "runner.sh"
CATALOG = ROOT / "scripts" / "local-dev-stack" / "service-catalog.sh"
PROFILES = ROOT / "scripts" / "local-dev-stack" / "profiles"


class LocalClientStackContractTests(unittest.TestCase):
    def run_services(self) -> str:
        completed = subprocess.run(
            ["bash", str(ENTRYPOINT), "services"],
            cwd=ROOT,
            check=True,
            text=True,
            capture_output=True,
        )
        return completed.stdout

    def test_entrypoints_and_profiles_are_valid_bash(self) -> None:
        paths = [
            ENTRYPOINT,
            COMMON_ENTRYPOINT,
            RUNNER,
            CATALOG,
            PROFILES / "full-development.sh",
            PROFILES / "local-client.sh",
        ]
        subprocess.run(["bash", "-n", *map(str, paths)], cwd=ROOT, check=True)

    def test_local_client_profile_has_only_final_client_services(self) -> None:
        output = self.run_services()
        required = {
            "configuration-center-backend",
            "user-service-backend",
            "memory-engine-backend",
            "memory-engine-worker",
            "plugin-management-backend",
            "chatos-backend",
        }
        removed = {
            "local-connector-service-backend",
            "mcp-management-service-backend",
            "official-website-backend",
            "admin-console-frontend",
            "official-website-frontend",
        }
        for name in required:
            self.assertIn(name, output)
        for name in removed:
            self.assertNotIn(name, output)

    def test_local_client_profile_reuses_the_common_runner(self) -> None:
        entrypoint = ENTRYPOINT.read_text()
        self.assertIn('STACK_PROFILE="local-client"', entrypoint)
        self.assertIn("local-dev-stack/runner.sh", entrypoint)
        self.assertNotIn("cargo build", entrypoint)
        self.assertNotIn("docker compose", entrypoint)
        profile = (PROFILES / "local-client.sh").read_text()
        self.assertNotIn("Cargo.toml", profile)
        self.assertNotIn("|/health|", profile)

    def test_empty_frontend_profile_supports_read_only_status(self) -> None:
        completed = subprocess.run(
            ["bash", str(ENTRYPOINT), "status"],
            cwd=ROOT,
            check=True,
            text=True,
            capture_output=True,
        )
        self.assertIn("ChatOS 3.0.2 local-client backend stack status", completed.stdout)

    def test_gateway_exposes_the_native_local_agent_model_surface(self) -> None:
        gateway = (ROOT / "docker" / "apisix" / "apisix.yaml").read_text()
        self.assertIn("id: chatos-model-gateway", gateway)
        self.assertIn("uri: /api/model-gateway/*", gateway)
        self.assertIn("id: memory-engine-sdk-api", gateway)
        self.assertIn("uri: /api/memory-engine/v1/*", gateway)
        self.assertIn('"chatos-backend:3997": 1', gateway)
        self.assertIn('"memory-engine-backend:7081": 1', gateway)

    def test_local_gateway_mounts_generated_runtime_files_outside_the_checkout(self) -> None:
        runner = RUNNER.read_text()
        environment = (ROOT / "scripts" / "local-dev-stack" / "environment.sh").read_text()
        compose = (ROOT / "docker" / "compose.platform.yml").read_text()
        local_override = (ROOT / "docker" / "compose.local-dev.yml").read_text()
        self.assertIn("/tmp/chatos-local-dev-${UID}/apisix", runner)
        self.assertIn("CHATOS_APISIX_RUNTIME_CONFIG_PATH", environment)
        self.assertIn("CHATOS_APISIX_RUNTIME_ROUTES_PATH", environment)
        self.assertIn("CHATOS_APISIX_RUNTIME_CONFIG_PATH", compose)
        self.assertIn("CHATOS_APISIX_RUNTIME_ROUTES_PATH", compose)
        self.assertNotIn("CHATOS_LOCAL_DEV_APISIX_CONFIG_PATH", local_override)


if __name__ == "__main__":
    unittest.main()
