# Native clients

ChatOS 3.0 uses two native shells over one shared client architecture.

- `shared/`: the authoritative Rust Local Agent Runtime, Host, Storage Provider, generated native contracts, golden fixtures, and cross-platform conformance rules.
- `macos/`: Swift 6.2/SwiftUI UI plus Keychain, Unix Socket, process lifecycle, and other macOS adapters.
- `windows/`: .NET 8/WinUI 3 UI plus DPAPI, Named Pipe, process lifecycle, and other Windows adapters.

Agent state, client business data, Task/Run projection, retry, context management, and tool semantics are implemented once under `shared/`. Platform directories must not contain a second Agent loop or independently reinterpret the shared state machine. Model configuration, Memory Engine, plugin management, authentication, and the stateless model gateway remain server APIs; Agent execution and plugin tools run locally.

Build and test commands are exposed from the repository `Makefile`; platform-specific packaging and installation instructions live in each client directory.

Generated apps, installers, MSIX packages, `bin`, `obj`, `.build`, local credentials, and runtime databases are not committed.
