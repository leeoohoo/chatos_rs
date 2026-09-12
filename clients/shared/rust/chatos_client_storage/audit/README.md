# Client storage access audit

`legacy_access.json` is the executable inventory for Work Package A. Every
entry with `status: pending` is an implementation site that must be removed or
rewired to the Local Agent Host before the storage work package is complete.

The inventory distinguishes structured business state from two allowed local
categories:

- ephemeral window and selection state stored in OS UI preferences;
- credentials stored in Keychain or Windows Credential Manager/DPAPI.

It is not a compatibility allowlist. `test_client_storage_boundary.py` rejects
new direct database-driver files immediately, and strict mode rejects every
remaining pending entry. Entries are deleted when their old implementation is
removed; they are never marked as permanent exceptions.

Run the normal guard:

```bash
python3 -m unittest scripts.tests.test_client_storage_boundary
```

Run the final Work Package A gate:

```bash
CHATOS_CLIENT_STORAGE_STRICT=1 python3 -m unittest scripts.tests.test_client_storage_boundary
```
