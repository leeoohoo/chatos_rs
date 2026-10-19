# Native clients

ChatOS 3.0 uses independent native clients that share cloud APIs, Realtime, Local Connector, plugin, and artifact protocols—not platform UI source code.

- `macos/`: Swift 6.2 and SwiftUI client with the native Local Connector, plugin applications, project tools, and macOS global productivity utilities.
- `windows-react/`: Tauri 2, React, and TypeScript replacement Windows client, rebuilt from the current macOS product behavior and Apple-style visual system.

Both clients keep local credentials and device capabilities on the user's machine while cloud services remain authoritative for project business data. Platform-specific behavior and parity are tested independently.

Build and test commands are exposed from the repository `Makefile`; platform-specific packaging and installation instructions live in each client directory.

Generated apps, installers, MSIX packages, `bin`, `obj`, `.build`, local credentials, and runtime databases are not committed.
