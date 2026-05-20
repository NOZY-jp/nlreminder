CREATE TABLE calendar_events (
    id TEXT PRIMARY KEY NOT NULL,
    external_uid TEXT UNIQUE,
    external_etag TEXT,
    title TEXT NOT NULL,
    starts_at TEXT,
    ends_at TEXT,
    state TEXT NOT NULL DEFAULT 'scheduled',
    remind_enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE todos (
    id TEXT PRIMARY KEY NOT NULL,
    external_task_id TEXT UNIQUE,
    title TEXT NOT NULL,
    due_at TEXT,
    state TEXT NOT NULL DEFAULT 'todo',
    remind_enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE mail_records (
    id TEXT PRIMARY KEY NOT NULL,
    message_id TEXT UNIQUE NOT NULL,
    subject TEXT,
    sender TEXT,
    received_at TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE reminder_plans (
    id TEXT PRIMARY KEY NOT NULL,
    target_type TEXT NOT NULL,
    target_id TEXT NOT NULL,
    scheduled_at TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_reminder_plans_active ON reminder_plans (status, scheduled_at);

CREATE TABLE reminder_statuses (
    id TEXT PRIMARY KEY NOT NULL,
    plan_id TEXT,
    discord_message_id TEXT,
    reaction TEXT NOT NULL DEFAULT 'no_response',
    notified_at TEXT NOT NULL,
    acknowledged_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (plan_id) REFERENCES reminder_plans (id)
);

CREATE TABLE llm_logs (
    id TEXT PRIMARY KEY NOT NULL,
    call_type TEXT NOT NULL,
    model TEXT NOT NULL,
    input_summary TEXT NOT NULL,
    output_json TEXT NOT NULL,
    duration_ms INTEGER NOT NULL,
    error TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE sync_state (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
