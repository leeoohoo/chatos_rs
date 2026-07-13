# ChatOS OpenAI Official Documentation

Use this Skill when the user asks about OpenAI APIs, models, SDKs, platform behavior, or current official guidance.

- Call `search_openai_docs` to discover current official sources from the Local Connector device.
- Call `extract_openai_docs` for the most relevant result URLs before making version-sensitive or behavioral claims.
- Extraction is restricted to HTTPS pages under `openai.com` and its subdomains. Do not use third-party pages as authoritative OpenAI documentation.
- Cite the official URLs included in the tool result. Clearly separate documented facts from your own implementation recommendation.
- If the official sources do not answer the question, say so instead of inventing an API or model capability.
- All network requests originate from the active Local Connector; this Skill does not use a server-side Codex manual or Codex-only documentation tool.
