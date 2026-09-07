CREATE TABLE schema_migrations (
    version INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL
);

INSERT INTO schema_migrations (version, applied_at)
VALUES (10000, CURRENT_TIMESTAMP);

CREATE TABLE project_info (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL,
    last_opened_at TEXT NOT NULL,
    data_format_version INTEGER NOT NULL CHECK (data_format_version = 10000)
);

CREATE TABLE datasets (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    source_type TEXT NOT NULL,
    source_filename TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE items (
    id TEXT PRIMARY KEY NOT NULL,
    dataset_id TEXT NOT NULL REFERENCES datasets(id) ON DELETE CASCADE,
    original_index INTEGER NOT NULL CHECK (original_index >= 0),
    fields_json TEXT NOT NULL CHECK (json_valid(fields_json)),
    is_valid INTEGER NOT NULL DEFAULT 1 CHECK (is_valid IN (0, 1)),
    created_at TEXT NOT NULL,
    UNIQUE (dataset_id, original_index)
);

CREATE TABLE field_definitions (
    id TEXT PRIMARY KEY NOT NULL,
    dataset_id TEXT NOT NULL REFERENCES datasets(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    field_type TEXT NOT NULL,
    display_order INTEGER NOT NULL CHECK (display_order >= 0),
    is_primary_identifier INTEGER NOT NULL DEFAULT 0 CHECK (is_primary_identifier IN (0, 1)),
    is_auxiliary_identifier INTEGER NOT NULL DEFAULT 0 CHECK (is_auxiliary_identifier IN (0, 1)),
    UNIQUE (dataset_id, name),
    CHECK (NOT (is_primary_identifier = 1 AND is_auxiliary_identifier = 1))
);

CREATE UNIQUE INDEX one_primary_identifier_per_dataset
ON field_definitions(dataset_id) WHERE is_primary_identifier = 1;

CREATE TABLE sort_tasks (
    id TEXT PRIMARY KEY NOT NULL,
    dataset_id TEXT NOT NULL REFERENCES datasets(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    criteria TEXT NOT NULL DEFAULT '',
    mode TEXT NOT NULL CHECK (mode IN ('drag', 'comparison', 'matrix', 'slider')),
    status TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'sorting', 'confirmed')),
    initial_order_type TEXT NOT NULL DEFAULT 'import'
        CHECK (initial_order_type IN ('import', 'field', 'task_result')),
    initial_order_field TEXT,
    initial_order_direction TEXT
        CHECK (initial_order_direction IS NULL OR initial_order_direction IN ('ascending', 'descending')),
    initial_order_task_id TEXT,
    matrix_comparison_percent INTEGER
        CHECK (matrix_comparison_percent IS NULL OR matrix_comparison_percent BETWEEN 1 AND 100),
    rank_written_at TEXT,
    rank_written_field TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE rank_groups (
    id TEXT PRIMARY KEY NOT NULL,
    sort_task_id TEXT NOT NULL REFERENCES sort_tasks(id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    initial_position INTEGER NOT NULL CHECK (initial_position >= 0),
    name TEXT,
    is_active INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    merged_into_group_id TEXT REFERENCES rank_groups(id),
    created_at TEXT NOT NULL,
    UNIQUE (sort_task_id, position)
);

CREATE TABLE rank_group_items (
    rank_group_id TEXT NOT NULL REFERENCES rank_groups(id) ON DELETE CASCADE,
    item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    item_order INTEGER NOT NULL DEFAULT 0 CHECK (item_order >= 0),
    PRIMARY KEY (rank_group_id, item_id),
    UNIQUE (rank_group_id, item_order)
);

CREATE TABLE comparisons (
    id TEXT PRIMARY KEY NOT NULL,
    sort_task_id TEXT NOT NULL REFERENCES sort_tasks(id) ON DELETE CASCADE,
    left_group_id TEXT NOT NULL REFERENCES rank_groups(id),
    right_group_id TEXT NOT NULL REFERENCES rank_groups(id),
    result TEXT NOT NULL CHECK (result IN ('left_before', 'right_before', 'tie')),
    source TEXT NOT NULL DEFAULT 'user' CHECK (source IN ('user', 'inferred')),
    is_undone INTEGER NOT NULL DEFAULT 0 CHECK (is_undone IN (0, 1)),
    session_state_before_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(session_state_before_json)),
    session_state_after_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(session_state_after_json)),
    group_state_before_json TEXT CHECK (group_state_before_json IS NULL OR json_valid(group_state_before_json)),
    group_state_after_json TEXT CHECK (group_state_after_json IS NULL OR json_valid(group_state_after_json)),
    created_at TEXT NOT NULL,
    CHECK (left_group_id <> right_group_id)
);

CREATE TABLE comparison_sessions (
    sort_task_id TEXT PRIMARY KEY NOT NULL REFERENCES sort_tasks(id) ON DELETE CASCADE,
    state_json TEXT NOT NULL CHECK (json_valid(state_json)),
    updated_at TEXT NOT NULL
);

CREATE TABLE matrix_sessions (
    sort_task_id TEXT PRIMARY KEY NOT NULL REFERENCES sort_tasks(id) ON DELETE CASCADE,
    state_json TEXT NOT NULL CHECK (json_valid(state_json)),
    updated_at TEXT NOT NULL
);

CREATE TABLE slider_ratings (
    sort_task_id TEXT NOT NULL REFERENCES sort_tasks(id) ON DELETE CASCADE,
    group_id TEXT NOT NULL REFERENCES rank_groups(id),
    item_id TEXT NOT NULL REFERENCES items(id),
    value_hundredths INTEGER NOT NULL CHECK (value_hundredths BETWEEN 0 AND 10000),
    completed_order INTEGER NOT NULL CHECK (completed_order >= 0),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (sort_task_id, group_id),
    UNIQUE (sort_task_id, item_id),
    UNIQUE (sort_task_id, completed_order)
);

CREATE TABLE score_configs (
    sort_task_id TEXT PRIMARY KEY NOT NULL REFERENCES sort_tasks(id) ON DELETE CASCADE,
    config_json TEXT NOT NULL CHECK (json_valid(config_json)),
    updated_at TEXT NOT NULL
);

CREATE TABLE snapshots (
    id TEXT PRIMARY KEY NOT NULL,
    sort_task_id TEXT NOT NULL REFERENCES sort_tasks(id) ON DELETE CASCADE,
    snapshot_type TEXT NOT NULL,
    state_json TEXT NOT NULL CHECK (json_valid(state_json)),
    created_at TEXT NOT NULL
);

CREATE TABLE operation_logs (
    id TEXT PRIMARY KEY NOT NULL,
    sort_task_id TEXT REFERENCES sort_tasks(id) ON DELETE CASCADE,
    operation_type TEXT NOT NULL,
    forward_payload_json TEXT NOT NULL CHECK (json_valid(forward_payload_json)),
    inverse_payload_json TEXT NOT NULL CHECK (json_valid(inverse_payload_json)),
    sequence INTEGER NOT NULL,
    is_undone INTEGER NOT NULL DEFAULT 0 CHECK (is_undone IN (0, 1)),
    created_at TEXT NOT NULL,
    UNIQUE (sort_task_id, sequence)
);

CREATE TABLE project_snapshots (
    id TEXT PRIMARY KEY NOT NULL,
    snapshot_type TEXT NOT NULL,
    label TEXT NOT NULL,
    file_name TEXT NOT NULL UNIQUE,
    source_task_id TEXT,
    source_dataset_id TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE project_runtime_state (
    singleton_id INTEGER PRIMARY KEY NOT NULL CHECK (singleton_id = 1),
    session_id TEXT,
    session_open INTEGER NOT NULL DEFAULT 0 CHECK (session_open IN (0, 1)),
    session_started_at TEXT,
    last_clean_exit_at TEXT,
    last_autosave_at TEXT,
    last_autosave_action TEXT
);

INSERT INTO project_runtime_state (singleton_id, session_open)
VALUES (1, 0);

CREATE TABLE media_assets (
    id TEXT PRIMARY KEY NOT NULL,
    dataset_id TEXT NOT NULL REFERENCES datasets(id) ON DELETE CASCADE,
    item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    field_name TEXT NOT NULL,
    source_kind TEXT NOT NULL CHECK (source_kind IN ('local', 'remote')),
    source_value TEXT NOT NULL,
    local_path TEXT,
    created_at TEXT NOT NULL,
    UNIQUE (item_id, field_name)
);

CREATE INDEX items_dataset_index ON items(dataset_id, original_index);
CREATE INDEX rank_groups_task_position ON rank_groups(sort_task_id, position);
CREATE INDEX rank_groups_task_initial_position ON rank_groups(sort_task_id, initial_position);
CREATE INDEX rank_groups_task_active_position ON rank_groups(sort_task_id, is_active, position);
CREATE INDEX comparisons_task_created ON comparisons(sort_task_id, created_at);
CREATE INDEX comparisons_task_active_created ON comparisons(sort_task_id, is_undone, created_at);
CREATE INDEX slider_ratings_task_value ON slider_ratings(sort_task_id, value_hundredths DESC, completed_order);
CREATE INDEX snapshots_task_created ON snapshots(sort_task_id, created_at);
CREATE INDEX operation_logs_task_sequence ON operation_logs(sort_task_id, sequence);
CREATE INDEX project_snapshots_created ON project_snapshots(created_at DESC);
CREATE INDEX media_assets_dataset ON media_assets(dataset_id, item_id);
