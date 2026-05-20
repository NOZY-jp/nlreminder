ALTER TABLE calendar_events ADD COLUMN external_href TEXT;
ALTER TABLE calendar_events ADD COLUMN all_day INTEGER NOT NULL DEFAULT 0;
ALTER TABLE calendar_events ADD COLUMN nlreminder_owned INTEGER NOT NULL DEFAULT 0;
ALTER TABLE calendar_events ADD COLUMN source_todo_id TEXT;

CREATE UNIQUE INDEX idx_reminder_plans_one_active
  ON reminder_plans (target_type, target_id)
  WHERE status = 'active';
