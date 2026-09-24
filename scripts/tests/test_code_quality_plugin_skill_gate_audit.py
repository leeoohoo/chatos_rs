import json
import subprocess
import tempfile
import unittest
from pathlib import Path


REPOSITORY_ROOT = Path(__file__).resolve().parents[2]
AUDITOR = REPOSITORY_ROOT / "scripts" / "audit-plugin-skill-gates.mjs"


class PluginSkillGateAuditTests(unittest.TestCase):
    def run_audit(self, tools):
        with tempfile.TemporaryDirectory(prefix="plugin-skill-audit-test-") as temporary:
            root = Path(temporary)
            (root / "skills" / "demo-router").mkdir(parents=True)
            (root / "skills" / "demo-leaf").mkdir(parents=True)
            (root / "skills" / "demo-router" / "SKILL.md").write_text(
                "---\nname: demo-router\ndescription: Route demo work.\nmetadata:\n"
                "  chatos.role: router\n---\n# Router\nChoose the required workflow.\n",
                encoding="utf-8",
            )
            (root / "skills" / "demo-leaf" / "SKILL.md").write_text(
                "---\nname: demo-leaf\ndescription: Perform demo work.\nmetadata:\n"
                "  chatos.role: leaf\n---\n# Leaf\nPerform one bounded operation.\n",
                encoding="utf-8",
            )
            (root / "chatos.plugin.json").write_text(
                json.dumps(
                    {
                        "name": "audit-fixture",
                        "skills": ["./skills/demo-router", "./skills/demo-leaf"],
                    }
                ),
                encoding="utf-8",
            )
            server = root / "server.mjs"
            server.write_text(
                "import readline from 'node:readline';\n"
                f"const tools = {json.dumps(tools)};\n"
                "const lines = readline.createInterface({ input: process.stdin });\n"
                "for await (const line of lines) {\n"
                "  const request = JSON.parse(line);\n"
                "  if (request.id === 1) console.log(JSON.stringify({jsonrpc:'2.0',id:1,result:{protocolVersion:'2025-06-18',capabilities:{tools:{}},serverInfo:{name:'fixture',version:'1'}}}));\n"
                "  if (request.id === 2) console.log(JSON.stringify({jsonrpc:'2.0',id:2,result:{tools}}));\n"
                "}\n",
                encoding="utf-8",
            )
            return subprocess.run(
                [
                    "node",
                    str(AUDITOR),
                    "--manifest",
                    str(root / "chatos.plugin.json"),
                    "--cwd",
                    str(root),
                    "--",
                    "node",
                    str(server),
                ],
                capture_output=True,
                check=False,
                text=True,
                timeout=10,
            )

    @staticmethod
    def tool(name, gate=None, properties=None, canonical=None):
        metadata = {}
        if gate is not None:
            metadata["chatos/skillGate"] = gate
        if canonical is not None:
            metadata["chatos/canonicalTool"] = canonical
        return {
            "name": name,
            "description": "Fixture tool",
            "inputSchema": {
                "type": "object",
                "properties": properties or {},
                "additionalProperties": False,
            },
            "_meta": metadata,
        }

    def test_accepts_complete_router_leaf_and_selector_coverage(self):
        result = self.run_audit(
            [
                self.tool(
                    "demo_run",
                    {
                        "allOf": ["demo-router"],
                        "selectByArgument": {
                            "pointer": "/mode",
                            "map": {"write": "demo-leaf"},
                        },
                    },
                    {"mode": {"type": "string", "enum": ["write"]}},
                )
            ]
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("1 tools, 2 Skills", result.stdout)

    def test_rejects_missing_gate_unknown_skill_and_internal_evidence(self):
        cases = [
            (
                [self.tool("missing_gate")],
                "missing _meta.chatos/skillGate",
            ),
            (
                [self.tool("unknown_skill", {"allOf": ["demo-router", "not-packaged"]})],
                "references unknown Skill not-packaged",
            ),
            (
                [
                    self.tool(
                        "leaks_evidence",
                        {"allOf": ["demo-router"]},
                        {"skillEvidence": {"type": "string"}},
                    )
                ],
                "exposes internal field skillEvidence",
            ),
        ]
        for tools, expected in cases:
            with self.subTest(expected=expected):
                result = self.run_audit(tools)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(expected, result.stderr)

    def test_rejects_alias_with_weaker_gate(self):
        result = self.run_audit(
            [
                self.tool("demo_write", {"allOf": ["demo-router", "demo-leaf"]}),
                self.tool(
                    "demo_write_legacy",
                    {"allOf": ["demo-router"]},
                    canonical="demo_write",
                ),
            ]
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("alias gate is weaker", result.stderr)


if __name__ == "__main__":
    unittest.main()
