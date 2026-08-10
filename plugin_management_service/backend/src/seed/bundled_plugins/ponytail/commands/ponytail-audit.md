# Ponytail audit

Audit the repository for unnecessary complexity without changing files. Rank the largest safe reductions first.

Use the tags `delete`, `stdlib`, `native`, `yagni`, and `shrink`. Each finding must name the replacement and source path. Do not recommend removing security, validation, error handling, accessibility, compatibility, migrations, audit, observability, or required tests.

End with `net: -<N> lines, -<M> dependencies possible.` If the repository is already lean, answer `Lean already. Ship.`
