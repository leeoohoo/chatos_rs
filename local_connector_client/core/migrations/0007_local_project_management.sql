CREATE TABLE IF NOT EXISTS project_profiles (
    project_id TEXT PRIMARY KEY,
    description TEXT,
    git_url TEXT,
    background TEXT,
    introduction TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(project_id) REFERENCES local_projects(project_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS project_requirements (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    owner_user_id TEXT NOT NULL,
    parent_requirement_id TEXT,
    requirement_type TEXT NOT NULL DEFAULT 'requirement',
    title TEXT NOT NULL,
    summary TEXT,
    detail TEXT,
    business_value TEXT,
    acceptance_criteria TEXT,
    source TEXT,
    priority INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'draft',
    creator_user_id TEXT,
    assignee_user_id TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    archived_at TEXT,
    FOREIGN KEY(project_id) REFERENCES local_projects(project_id) ON DELETE CASCADE,
    FOREIGN KEY(parent_requirement_id) REFERENCES project_requirements(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_project_requirements_project_updated
ON project_requirements(project_id, updated_at DESC);

CREATE TABLE IF NOT EXISTS requirement_documents (
    id TEXT PRIMARY KEY,
    requirement_id TEXT NOT NULL,
    owner_user_id TEXT NOT NULL,
    doc_type TEXT NOT NULL,
    title TEXT NOT NULL,
    format TEXT NOT NULL DEFAULT 'markdown',
    content TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(requirement_id) REFERENCES project_requirements(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_requirement_documents_requirement_updated
ON requirement_documents(requirement_id, updated_at DESC);

CREATE TABLE IF NOT EXISTS requirement_dependencies (
    requirement_id TEXT NOT NULL,
    prerequisite_requirement_id TEXT NOT NULL,
    relation_type TEXT NOT NULL DEFAULT 'blocks',
    created_at TEXT NOT NULL,
    PRIMARY KEY(requirement_id, prerequisite_requirement_id),
    FOREIGN KEY(requirement_id) REFERENCES project_requirements(id) ON DELETE CASCADE,
    FOREIGN KEY(prerequisite_requirement_id) REFERENCES project_requirements(id) ON DELETE CASCADE,
    CHECK(requirement_id <> prerequisite_requirement_id)
);

CREATE TABLE IF NOT EXISTS project_work_items (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    requirement_id TEXT NOT NULL,
    owner_user_id TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT,
    status TEXT NOT NULL DEFAULT 'todo',
    priority INTEGER NOT NULL DEFAULT 0,
    assignee_user_id TEXT,
    estimate_points INTEGER,
    due_at TEXT,
    sort_order INTEGER NOT NULL DEFAULT 0,
    tags_json TEXT NOT NULL DEFAULT '[]',
    is_planning_task INTEGER NOT NULL DEFAULT 0,
    creator_user_id TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    archived_at TEXT,
    FOREIGN KEY(project_id) REFERENCES local_projects(project_id) ON DELETE CASCADE,
    FOREIGN KEY(requirement_id) REFERENCES project_requirements(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_project_work_items_project_updated
ON project_work_items(project_id, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_project_work_items_requirement_order
ON project_work_items(requirement_id, sort_order ASC, created_at ASC);

CREATE TABLE IF NOT EXISTS work_item_dependencies (
    work_item_id TEXT NOT NULL,
    prerequisite_work_item_id TEXT NOT NULL,
    relation_type TEXT NOT NULL DEFAULT 'blocks',
    created_at TEXT NOT NULL,
    PRIMARY KEY(work_item_id, prerequisite_work_item_id),
    FOREIGN KEY(work_item_id) REFERENCES project_work_items(id) ON DELETE CASCADE,
    FOREIGN KEY(prerequisite_work_item_id) REFERENCES project_work_items(id) ON DELETE CASCADE,
    CHECK(work_item_id <> prerequisite_work_item_id)
);
