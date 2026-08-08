# ChatOS adaptation notice

This package adapts Ponytail 4.8.4 by Dietrich Gebert for ChatOS.

The MIT-licensed core guidance is retained. ChatOS changes the wording from shortest code to the minimum maintainable correct change, follows repository test and quality requirements, and explicitly preserves security, validation, accessibility, compatibility, migrations, auditability, and observability.

The upstream Node lifecycle Hooks, MCP server, local mode files, environment configuration, and persistent `/ponytail off` behavior are intentionally not included. ChatOS activates the signed Prompt components through its Plugin Runtime and controls intensity through Agent Profiles.
