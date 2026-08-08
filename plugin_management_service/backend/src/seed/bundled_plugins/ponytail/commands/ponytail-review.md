# Ponytail review

Review the current diff only for unnecessary complexity. Do not apply fixes.

One finding per line: `<file>:L<line>: <tag> <what to cut>. <replacement>.`

Tags: `delete`, `stdlib`, `native`, `yagni`, `shrink`.

Do not report correctness, security, accessibility, or performance findings as complexity findings. End with `net: -<N> lines possible.` If nothing should be cut, answer `Lean already. Ship.`
