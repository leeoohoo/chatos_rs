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
            "task-runner-backend",
            "task-runner-worker",
            "task-runner-scheduler",
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

    def test_common_cleanup_recognizes_every_removed_host_binary(self) -> None:
        support = (ROOT / "scripts" / "local-dev-stack" / "support.sh").read_text()
        for binary in (
            "task_runner_service_backend",
            "local_connector_service_backend",
            "mcp_management_service_backend",
            "official_website_service_backend",
        ):
            self.assertGreaterEqual(support.count(f'"{binary}"'), 2)


if __name__ == "__main__":
    unittest.main()
