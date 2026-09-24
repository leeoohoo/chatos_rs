# Project read scenarios

## Locate an implementation

Good: search for the stable symbol or literal within the likely source directory, then read narrow ranges around the strongest matches.

Bad: read every file in the repository or run broad shell pipelines before using the project search tools.

## Large file

Good: locate relevant lines first and use `read_file_range` with explicit 1-based bounds. Expand only when surrounding context is needed.

Bad: repeatedly request the entire file and overload model context, or assume an omitted tail does not exist.

## Empty search result

Good: check the searched path, fixed-text spelling, ignored generated/dependency directories, and result limits before concluding the text is absent.

Bad: claim there are no references without stating the searched scope or whether results were truncated.

## Ambiguous path

Good: list the parent directory and select an exact returned entry. Preserve case and avoid inventing missing path components.

Bad: guess between similarly named files or use an absolute path outside the project.

## Prepare a safe edit

Good: read the exact target and retain its hash or stable surrounding text before opening a transactional edit session.

Bad: stage a replacement from memory after earlier unrelated changes may have altered the file.

## Open a file in the pet workbench

Good: use `open_file_in_pet` only after an explicit Human request, choose preview or edit mode as requested, and include a line only when it helps navigation.

Bad: open UI merely because the model needed to inspect a file internally, or use workbench opening as proof that file contents were validated.
