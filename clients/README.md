# Native clients

ChatOS 3.0 uses independent native clients that share server protocols and product behavior, not platform source code.

- `macos/`: Swift 6.2 and SwiftUI client with the native Local Connector.
- `windows/`: .NET 8, C#, WinUI 3 client with the native Local Connector and Network Guard.

Build and test commands are exposed from the repository `Makefile`; platform-specific packaging and installation instructions live in each client directory.

Generated apps, MSIX packages, `bin`, `obj`, `.build`, local credentials, and runtime databases are not committed.
