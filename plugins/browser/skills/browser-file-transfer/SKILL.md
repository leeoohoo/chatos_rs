---
name: browser-file-transfer
description: Upload user-authorized local files and verify browser downloads as managed artifacts without inventing paths or bypassing file grants.
metadata:
  chatos.role: leaf
---

# Browser file transfer

Use only files explicitly selected, attached, or granted by the user and host. Never guess absolute paths or broaden a file grant. Match the file input or chooser using a fresh snapshot, perform the upload, and verify the page shows the intended filename or completed state.

For downloads, trigger the requested download once, wait for completion, and return the managed artifact metadata supplied by the plugin. Do not claim that a file exists merely because a link was clicked. Avoid repeated clicks that create duplicate downloads.

Read [file-transfer examples](references/examples.md) for chooser and download verification patterns.
