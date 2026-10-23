# Focus examples

## Positive

Inspect the current `activate_application` schema. On macOS, activate the known `bundle_identifier`; on Windows, activate the known `application`. Verify the returned identity and visible window, click the intended editor, verify focus, then type.

## Negative

Send a save shortcut because the requested app was active several screenshots ago. Another application or dialog may now own the keyboard.

If typing appears in the wrong place, stop. Observe the full display, identify the actual foreground app and modal state, then reacquire the intended field instead of sending compensating shortcuts blindly.
