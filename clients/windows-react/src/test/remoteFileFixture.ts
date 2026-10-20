import type { RemoteDirectoryListing, RemoteFileEntry } from "../features/remote-files/domain/remoteFiles";

export function remoteFileEntryFixture(overrides: Partial<RemoteFileEntry> = {}): RemoteFileEntry {
  return {
    name: "app.log",
    path: "/srv/app/app.log",
    kind: "file",
    size: 2_048,
    modifiedAtEpochSeconds: 1_700_000_000,
    permissions: "-rw-r--r--",
    ...overrides,
  };
}

export function remoteDirectoryFixture(overrides: Partial<RemoteDirectoryListing> = {}): RemoteDirectoryListing {
  return {
    path: "/srv/app",
    parentPath: "/srv",
    entries: [
      remoteFileEntryFixture({ name: "releases", path: "/srv/app/releases", kind: "directory", size: null, permissions: "drwxr-xr-x" }),
      remoteFileEntryFixture(),
      remoteFileEntryFixture({ name: ".env", path: "/srv/app/.env", size: 120 }),
    ],
    truncated: false,
    ...overrides,
  };
}
