# File-transfer examples

## Positive: upload

Use the user-granted file, open the current file control, submit the exact file, and verify the visible filename or upload-complete state.

## Negative: path guessing

Construct `/Users/.../report.pdf` from a conversation hint. Local file access must come from a trusted grant, not model inference.

## Positive: download

Click once, wait for the plugin's completed download result, and provide the returned managed artifact.

## Negative: click equals file

Report a downloadable file immediately after clicking while the browser is still negotiating, blocked by a popup, or awaiting confirmation.
