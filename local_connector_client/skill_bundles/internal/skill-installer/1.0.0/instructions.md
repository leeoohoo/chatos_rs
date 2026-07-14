# ChatOS Trusted Skill Installer

Use this Skill for the installation and validation workflow of ChatOS Skill Bundles.

Current release rules:

- Only bundles shipped with the Local Connector installer are trusted.
- A bundle is identified by bundle ID, immutable version, and content hash.
- The local runtime must reject hash mismatches, unsupported platforms, missing dependencies, and incomplete adapters.
- Installation or verification never sends bundle files, credentials, or workspace content to the cloud.
- A verified bundle is still disabled for a user until that user explicitly enables it in Local Connector settings.
- Never execute code from an arbitrary Git URL, ZIP file, or marketplace entry in the current release.

When a requested install source is not an internal bundled source, explain that user installation is not yet enabled and do not bypass the restriction.
