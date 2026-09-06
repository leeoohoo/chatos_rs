# Network debugging examples

## Positive: failed API call

Reproduce once, filter to the relevant host/path, inspect status and safe response metadata, correlate with the visible failure, then report the failing boundary and evidence.

## Negative: collect everything

Capture all network traffic and dump every header and body. This increases noise and can expose credentials unrelated to the task.

## Positive: temporary interception

With authorization, add one narrowly matched rule, trigger one request, verify the simulated state, then remove the rule.

## Negative: unexplained raw CDP

Send arbitrary CDP methods because they are available. Raw access is a fallback, not the default workflow.
