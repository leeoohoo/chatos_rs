from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[2]

OLD_FRONTEND_DIRS = (
    "config_center_service/frontend",
    "user_service/frontend",
    "memory_engine/frontend",
    "project_management_service/frontend",
    "plugin_management_service/frontend",
    "task_runner_service/frontend",
)

OLD_FRONTEND_SERVICES = (
    "configuration-center-frontend",
    "user-service-frontend",
    "memory-engine-frontend",
    "project-management-frontend",
    "plugin-management-frontend",
    "task-runner-frontend",
)


class UnifiedAdminTopologyTests(unittest.TestCase):
    def test_old_frontend_projects_are_removed(self) -> None:
        for relative_path in OLD_FRONTEND_DIRS:
            self.assertFalse((ROOT / relative_path).exists(), relative_path)

    def test_compose_uses_only_the_unified_admin_frontend(self) -> None:
        compose = (ROOT / "docker/compose.yml").read_text()
        build = (ROOT / "docker/compose.build.yml").read_text()
        for content in (compose, build):
            self.assertIn("admin-console-frontend:", content)
            for service in OLD_FRONTEND_SERVICES:
                self.assertNotIn(f"{service}:", content)

    def test_gateway_exposes_six_scoped_admin_apis(self) -> None:
        config = (ROOT / "docker/apisix/apisix.yaml").read_text()
        for service in (
            "user-service",
            "project-service",
            "task-runner",
            "plugin-management",
            "memory-engine",
            "config-center",
        ):
            self.assertIn(f"/api/admin/{service}/*", config)
        self.assertIn("uri-blocker: &block_internal_api", config)
        self.assertIn("uri-blocker: *block_internal_api", config)
        self.assertIn("hosts: &admin_hosts", config)
        self.assertIn("hosts: *admin_hosts", config)
        self.assertIn("admin/(?:user-service|project-service|task-runner", config)
        self.assertIn("(?:chatos|user|project|plugin|plugins|task|memory|local)", config)
        self.assertIn("(?:api/)?internal(?:/|\\\\?|$)", config)
        self.assertIn("admin.jgoool.com", config)
        self.assertIn('"admin-console-frontend:80"', config)

    def test_sandbox_manager_public_host_is_gone(self) -> None:
        for relative_path in (
            "docker/apisix/apisix.yaml",
            "docker/nginx/jgoool-http.conf",
            "docker/nginx/jgoool-https.conf",
            "scripts/local-dev-stack/services.sh",
        ):
            content = (ROOT / relative_path).read_text()
            self.assertNotIn("sandbox.jgoool.com", content)
            self.assertNotIn("sandbox-runtime-proxy", content)


if __name__ == "__main__":
    unittest.main()
