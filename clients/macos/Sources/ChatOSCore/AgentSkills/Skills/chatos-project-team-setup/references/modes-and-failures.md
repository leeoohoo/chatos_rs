# Project team setup modes and failures

## Existing ChatOS project

Good: call `project_catalog`, select a returned `project_option` whose project has no active team, and submit one team name and goal.

Bad: invent an option, reuse one from another run, or propose a second active team for an occupied project.

## New managed project

Good: use `team_propose_new_project` only when the Human asked ChatOS to create a new project. Choose a valid project type and describe the team goal separately from the project description.

Bad: use the new-project mode merely because a named project was not found without checking spelling or clarifying the Human's intent.

## Import an existing local directory

Good: preserve the absolute directory exactly as the Human supplied it and use the import mode only when the directory already exists inside an authorized workspace.

Bad: convert `https://github.com/org/repo`, `git@github.com:org/repo.git`, a repository slug, or a relative path into a guessed absolute path.

## Remote repository work

Good: create or update a Todo with the terminal capability when the request is to clone, download, inspect, build, run, or analyze a remote repository.

Bad: create a ChatOS project/team proposal solely because the request contains the word “project” or a Git URL.

## Authorized execution

Good: treat a successful `approved` tool result as an executed change, re-read the current state, and tell the Human what was created.

Bad: ask the Human to approve the same authorized action again, or claim success after an error result.

## Conflict or stale catalog

Good: refresh `project_catalog`, explain the changed state, and submit a new proposal only if it still matches the Human's request.

Bad: retry the same stale option or switch setup modes to force a proposal through.
