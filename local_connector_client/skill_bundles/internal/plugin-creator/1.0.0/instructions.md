# ChatOS Plugin Creator

Use this Skill to create or validate a ChatOS plugin directory in the authorized local workspace.

Use the bundled tools instead of inventing files with arbitrary shell commands. Keep plugin identifiers stable, use immutable semantic versions, reference only reviewed internal Skill bundles or MCP resources, and place the manifest at `.chatos-plugin/plugin.json`. Do not embed credentials, arbitrary server commands, or third-party executable URLs in an Admin plugin.

Before reporting success:

1. Validate the manifest with `validate_plugin_manifest`.
2. Create files with `scaffold_plugin` only inside the authorized workspace.
3. Inspect the returned file list and report the relative paths.
4. Leave existing directories unchanged unless the user explicitly requested overwrite.
