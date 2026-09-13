import pathlib
import subprocess
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
PACKAGE_SCRIPT = ROOT / "clients" / "macos" / "scripts" / "package-debug-app.sh"
SETUP_SCRIPT = (
    ROOT / "clients" / "macos" / "scripts" / "setup-local-signing-identity.sh"
)
APP_CREDENTIAL_STORE = (
    ROOT
    / "clients"
    / "macos"
    / "Sources"
    / "ChatOSApp"
    / "Infrastructure"
    / "KeychainCredentialStore.swift"
)
AGENT_CREDENTIAL_STORE = (
    ROOT
    / "clients"
    / "macos"
    / "Sources"
    / "ChatOSConnector"
    / "NativeLocalAgentCredentialStore.swift"
)
KEYCHAIN_BROKER = (
    ROOT
    / "clients"
    / "macos"
    / "Sources"
    / "ChatOSKeychainBroker"
    / "main.swift"
)
KEYCHAIN_BROKER_CLIENT = (
    ROOT
    / "clients"
    / "macos"
    / "Sources"
    / "ChatOSConnector"
    / "MacOSKeychainBrokerClient.swift"
)
MACOS_CODE_IDENTITY = (
    ROOT
    / "clients"
    / "macos"
    / "Sources"
    / "ChatOSMacSecurity"
    / "CodeIdentity.swift"
)


class MacOSLocalSigningContractTests(unittest.TestCase):
    def test_signing_scripts_are_valid_zsh(self) -> None:
        subprocess.run(
            ["/bin/zsh", "-n", str(PACKAGE_SCRIPT), str(SETUP_SCRIPT)],
            cwd=ROOT,
            check=True,
        )

    def test_debug_packages_use_a_dedicated_stable_identity(self) -> None:
        package = PACKAGE_SCRIPT.read_text()
        setup = SETUP_SCRIPT.read_text()

        for expected in (
            "ChatOS Local Development",
            "signing.keychain-db",
            "keychain-password",
            "CODESIGN_KEYCHAIN_ARGUMENTS",
            "security unlock-keychain",
            "ChatOSKeychainBroker",
            "chatos_keychain_broker",
            "com.chatos.swift-client.keychain-broker",
        ):
            self.assertIn(expected, package)

        self.assertNotIn('codesign --force --sign -', package)
        self.assertIn("security create-keychain", setup)
        self.assertIn("security set-key-partition-list", setup)
        self.assertIn("-T /usr/bin/codesign", setup)
        self.assertIn("-p codeSign", setup)

    def test_keychain_reads_never_request_interactive_ui(self) -> None:
        source = KEYCHAIN_BROKER.read_text()
        self.assertIn("interactionNotAllowed = true", source)
        self.assertIn("kSecUseAuthenticationContext", source)
        self.assertIn("kSecUseAuthenticationUI as String", source)
        self.assertIn("kSecUseAuthenticationUIFail", source)

    def test_package_reopens_signing_keychain_after_compilation_without_ui(self) -> None:
        package = PACKAGE_SCRIPT.read_text()

        build_position = package.index("cargo build -p chatos_local_agent_host")
        final_unlock_position = package.rindex("prepare_local_signing_keychain")
        first_codesign_position = package.index("codesign \\\n")

        self.assertGreater(final_unlock_position, build_position)
        self.assertLess(final_unlock_position, first_codesign_position)
        self.assertIn("security set-key-partition-list", package)
        self.assertIn("trap lock_local_signing_keychain EXIT", package)
        self.assertIn('security lock-keychain "$LOCAL_SIGNING_KEYCHAIN"', package)

    def test_runtime_uses_fresh_non_legacy_credential_namespaces(self) -> None:
        app_source = APP_CREDENTIAL_STORE.read_text()
        agent_source = AGENT_CREDENTIAL_STORE.read_text()
        self.assertIn("com.chatos.swift-client.authentication.v6", app_source)
        self.assertNotIn("com.chatos.swift-client.authentication.v5", app_source)
        self.assertIn('productionService = "com.chatos.local-agent.credentials.v7"', agent_source)
        self.assertNotIn('productionService = "com.chatos.local-agent.credentials.v6"', agent_source)

    def test_production_broker_is_installed_once_outside_replaceable_app_bundle(self) -> None:
        client = KEYCHAIN_BROKER_CLIENT.read_text()
        broker = KEYCHAIN_BROKER.read_text()
        identity = MACOS_CODE_IDENTITY.read_text()

        self.assertIn('appendingPathComponent("KeychainBrokerV3"', client)
        self.assertIn("installStableBroker(from: bundled, to: installed)", client)
        self.assertIn("if fileManager.fileExists(atPath: destination.path)", client)
        self.assertIn("isTrustedProductionBroker(at: destination)", client)
        self.assertIn("secureRegularFile(at: url)", client)
        self.assertIn('appIdentity.identifier == "com.chatos.swift-client"', client)
        self.assertIn(
            'brokerIdentity.identifier == "com.chatos.swift-client.keychain-broker"',
            client,
        )
        self.assertIn("brokerIdentity.leafCertificateData == appIdentity.leafCertificateData", client)
        self.assertIn("MacOSCodeSigning.identity(forProcessID: parentProcessID)", broker)
        self.assertIn("import ChatOSMacSecurity", broker)
        self.assertIn("SecStaticCodeCheckValidity", identity)


if __name__ == "__main__":
    unittest.main()
