ALTER TABLE project_runtime_environment_images
ADD COLUMN source_root TEXT NOT NULL DEFAULT '.';

ALTER TABLE project_runtime_environment_images
ADD COLUMN component_kind TEXT NOT NULL DEFAULT '';

ALTER TABLE project_runtime_environment_images
ADD COLUMN startup_command TEXT;

ALTER TABLE project_runtime_environment_images
ADD COLUMN test_command TEXT;

ALTER TABLE project_runtime_environment_images
ADD COLUMN depends_on_json TEXT NOT NULL DEFAULT '[]';

ALTER TABLE project_runtime_environment_images
ADD COLUMN auto_start INTEGER NOT NULL DEFAULT 0;
