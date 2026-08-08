# Ponytail debt ledger

Read and report every source comment matching a real comment prefix followed by `ponytail:`. Skip `.git`, dependency directories, generated output, and build output. Do not modify files.

Group by file and output: `<file>:<line>, <simplification>. ceiling: <limit>. upgrade: <trigger>.` Mark entries without a concrete trigger as `no-trigger`.

End with `<N> markers, <M> with no trigger.` If none exist, answer `No ponytail: debt. Clean ledger.`
