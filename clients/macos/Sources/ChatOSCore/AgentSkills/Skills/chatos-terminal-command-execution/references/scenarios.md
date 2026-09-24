# Command execution scenarios

## Fast deterministic verification

Situation: run a focused test expected to finish in under a minute.

Good: execute it in the relevant project directory in foreground mode, retain the exit status, and cite the failing test or passing summary.

Bad: start it in the background and immediately claim success, or run a repository-wide suite when the task only needs one focused test.

## Build with an uncertain duration

Situation: a clean build may exceed the normal foreground window.

Good: use background mode, capture `terminal_id`, then use process observation in bounded intervals. If the build exits, read enough final log context to distinguish a compiler failure from cancellation or timeout.

Bad: request the maximum foreground timeout repeatedly. This can strand the run in long waits and may duplicate expensive work.

## Development server or watcher

Situation: start a server that is expected to remain alive.

Good: use background mode, poll until a readiness line or a failed exit appears, and leave it running only when the task requires it.

Bad: treat “still running” as readiness, or use a foreground call that can only end by timing out.

## Retry after unclear transport or timeout result

Situation: the command call did not return a clear completion result.

Good: inspect the known process list and recent logs first. Retry only when the earlier attempt is proven absent or safe to repeat.

Bad: immediately rerun database migration, publish, install, or file-generation commands. The first attempt may have completed and a duplicate can corrupt state or waste resources.

## User-provided data in a command

Situation: search for a user-provided string containing spaces or shell punctuation.

Good: validate the value and use robust quoting or a dedicated search/file tool.

Bad: concatenate the raw value into a pipeline. Characters such as `;`, `$()`, globs, or redirections may change the command meaning.

## Expected failure used as a diagnostic

Situation: a probe intentionally checks whether a file, tool, or service is absent.

Good: preserve and interpret its exit status explicitly, then explain why that status is expected.

Bad: append an unconditional success clause that erases the difference between the expected condition and an unrelated execution failure.
