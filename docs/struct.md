# nlreminder 構造体仕様

ドメイン型・DB 行・LLM 向け View・設定の **型定義の正本**。要件は [`_nlreminder.md`](_nlreminder.md)、API 呼び出しは [`api.md`](api.md) を参照。

**実装場所:** `src/models/`（ドメイン）、既存モジュール内 DTO（`CalDavEvent` 等）は移行期間中そのまま。

---

## 1. 共通規約

| 項目 | 規約 |
|------|------|
| 内部 ID | `Uuid` v4 → DB / JSON では **小文字ハイフン付き** `String` |
| 日時 | Rust: `DateTime<Utc>`。DB / JSON: **RFC 3339** `TEXT` |
| 真偽値 | Rust: `bool`。DB: `INTEGER` `0` / `1` |
| 列挙 | Rust: `enum` + `strum`。DB / JSON: **snake_case** 文字列 |
| NULL | 外部 ID 未同期時のみ `Option`。`title` 等必須列は NULL 不可 |

---

## 2. 設定

### `EnvConfig`（`.env`）— 実装済み `src/config/env.rs`

| フィールド | 型 | 備考 |
|-----------|-----|------|
| `google_client_id` | `String` | |
| `google_client_secret` | `String` | |
| `google_refresh_token` | `Option<String>` | 運用時必須 |
| `google_account_email` | `String` | |
| `lmstudio_model` | `String` | 既定 `Qwen3.6-35B` |
| `lmstudio_base_url` | `String` | 既定 `http://localhost:1234/v1` |
| `lmstudio_timeout_secs` | `u64` | 既定 `120`（**読み込み未実装**） |
| `caldav_url` | `String` | |
| `caldav_username` | `String` | |
| `caldav_password` | `String` | |
| `discord_token` | `String` | |
| `discord_guild_id` | `String` | |
| `discord_channel_id` | `String` | |
| `oauth_callback_port` | `u16` | 既定 `8080` |

### `Settings`（`config.toml`）— 実装済み `src/config/settings.rs`

| フィールド | 型 | 既定 |
|-----------|-----|------|
| `timezone` | `String` | `Asia/Tokyo` |
| `morning_summary_hour` | `u32` | `8` |
| `quiet_hours_start` | `u32` | `0` |
| `quiet_hours_end` | `u32` | `8` |
| `scan_interval_secs` | `u64` | `300` |
| `database_path` | `PathBuf` | `data/nlreminder.db` |
| `backup_dir` | `PathBuf` | `backups` |
| `google_tasks_list_id` | `String` | `""` → `@default` |
| `caldav_calendar_path` | `String` | `"/nozyjp/sshCalendar/"` |

### `AppConfig`

```rust
struct AppConfig {
    env: EnvConfig,
    settings: Settings,
}
```

---

## 3. 列挙型

### `CalendarEventState`

| 値 (DB/JSON) | Rust variant | 意味 |
|--------------|--------------|------|
| `scheduled` | `Scheduled` | 予定 |
| `prepared` | `Prepared` | 準備完了 |
| `completed` | `Completed` | 完了 |

### `TodoState`

| 値 | variant | 意味 |
|----|---------|------|
| `todo` | `Todo` | 未着手 |
| `ongoing` | `Ongoing` | 進行中 |
| `done` | `Done` | 完了 |

### `ReminderTargetType`

| 値 | variant |
|----|---------|
| `calendar_event` | `CalendarEvent` |
| `todo` | `Todo` |

### `ReminderPlanStatus`

| 値 | variant |
|----|---------|
| `active` | `Active` |
| `cancelled` | `Cancelled` |
| `completed` | `Completed` |

### `ReminderReaction`

| 値 | variant |
|----|---------|
| `no_response` | `NoResponse` |
| `acknowledged` | `Acknowledged` |

### `LlmCallType`

| 値 | 用途 |
|----|------|
| `morning_summary` | 朝サマリー |
| `single_reminder` | 随時リマインド |
| `mail_classify` | メール分類 |
| `ignore_evaluate` | 無視候補判定 |
| `plan_create` | リマインド計画 |
| `discord_intent` | Discord 意図解析 |

### `MailCategory`（LLM 出力のみ）

| 値 | notify_in_summary | persist |
|----|-------------------|---------|
| `payment` | true | false |
| `outreach` | true | true |
| `assignment_deadline` | true | false |
| `assignment_submitted` | false | false |
| `other` | false | false |

### `ReminderUrgency`（LLM 出力）

`low` | `normal` | `high`

### `IgnoreJudgment` / `IgnoreAdjustment`

- judgment: `not_seen` | `ignored`
- adjustment: `keep` | `delay` | `intensify`

### `DiscordIntent`

`add_calendar` | `add_todo` | `update_state` | `acknowledge` | `snooze` | `exclude_reminder` | `chat`

---

## 4. DB ドメインモデル（`src/models/`）

### `CalendarEvent` — テーブル `calendar_events`

**migration 001（現行）**

| フィールド | 型 | DB 列 | 備考 |
|-----------|-----|-------|------|
| `id` | `Uuid` | `id` PK | |
| `external_uid` | `Option<String>` | `external_uid` UNIQUE | iCalendar UID |
| `external_etag` | `Option<String>` | `external_etag` | 楽観ロック |
| `title` | `String` | `title` | |
| `starts_at` | `Option<DateTime<Utc>>` | `starts_at` | |
| `ends_at` | `Option<DateTime<Utc>>` | `ends_at` | |
| `state` | `CalendarEventState` | `state` | 既定 `scheduled` |
| `remind_enabled` | `bool` | `remind_enabled` | 既定 `true` |
| `created_at` | `DateTime<Utc>` | `created_at` | |
| `updated_at` | `DateTime<Utc>` | `updated_at` | |

**migration 002（追加予定）**

| フィールド | 型 | DB 列 | 備考 |
|-----------|-----|-------|------|
| `external_href` | `Option<String>` | `external_href` | CalDAV オブジェクト URL |
| `all_day` | `bool` | `all_day` | 既定 `false` |
| `nlreminder_owned` | `bool` | `nlreminder_owned` | `TODO:` 由来等。既定 `false` |
| `source_todo_id` | `Option<Uuid>` | `source_todo_id` | 締切連動イベントの逆参照 |

**インデックス（002）:** `(external_uid)` は UNIQUE 維持。

**CalDAV から消えた行:** 削除せず `state = completed`（§9.8）。

**nlreminder 作成イベントの UID 規則:**

- 手動 / Discord 追加: 新規 UUID を iCalendar UID に使用
- Todo 締切連動: `nlreminder-todo-{todo_id}`（固定。再同期の idempotency）

---

### `Todo` — テーブル `todos`

| フィールド | 型 | DB 列 | 備考 |
|-----------|-----|-------|------|
| `id` | `Uuid` | `id` PK | |
| `external_task_id` | `Option<String>` | `external_task_id` UNIQUE | Google Task ID |
| `title` | `String` | `title` | |
| `due_at` | `Option<DateTime<Utc>>` | `due_at` | |
| `state` | `TodoState` | `state` | 既定 `todo` |
| `remind_enabled` | `bool` | `remind_enabled` | 既定 `true` |
| `created_at` | `DateTime<Utc>` | `created_at` | |
| `updated_at` | `DateTime<Utc>` | `updated_at` | |

**Google Tasks `@default` リストのみ**（`google_tasks_list_id` 空）。

---

### `MailRecord` — テーブル `mail_records`

| フィールド | 型 | DB 列 |
|-----------|-----|-------|
| `id` | `Uuid` | `id` PK |
| `message_id` | `String` | `message_id` UNIQUE |
| `subject` | `Option<String>` | `subject` |
| `sender` | `Option<String>` | `sender` |
| `received_at` | `DateTime<Utc>` | `received_at` |
| `created_at` | `DateTime<Utc>` | `created_at` |

---

### `ReminderPlan` — テーブル `reminder_plans`

| フィールド | 型 | DB 列 |
|-----------|-----|-------|
| `id` | `Uuid` | `id` PK |
| `target_type` | `ReminderTargetType` | `target_type` |
| `target_id` | `Uuid` | `target_id` |
| `scheduled_at` | `DateTime<Utc>` | `scheduled_at` |
| `status` | `ReminderPlanStatus` | `status` |
| `created_at` | `DateTime<Utc>` | `created_at` |
| `updated_at` | `DateTime<Utc>` | `updated_at` |

**制約:** `(target_type, target_id)` に **`status = active` は最大 1 件**（アプリ層 + migration 002 で partial UNIQUE INDEX）。

```sql
CREATE UNIQUE INDEX idx_reminder_plans_one_active
  ON reminder_plans (target_type, target_id)
  WHERE status = 'active';
```

---

### `ReminderStatus` — テーブル `reminder_statuses`

| フィールド | 型 | DB 列 |
|-----------|-----|-------|
| `id` | `Uuid` | `id` PK |
| `plan_id` | `Option<Uuid>` | `plan_id` FK |
| `discord_message_id` | `Option<String>` | `discord_message_id` |
| `reaction` | `ReminderReaction` | `reaction` |
| `notified_at` | `DateTime<Utc>` | `notified_at` |
| `acknowledged_at` | `Option<DateTime<Utc>>` | `acknowledged_at` |
| `created_at` | `DateTime<Utc>` | `created_at` |
| `updated_at` | `DateTime<Utc>` | `updated_at` |

---

### `LlmLog` — テーブル `llm_logs`

| フィールド | 型 | DB 列 |
|-----------|-----|-------|
| `id` | `Uuid` | `id` PK |
| `call_type` | `LlmCallType` | `call_type` |
| `model` | `String` | `model` |
| `input_summary` | `String` | `input_summary` |
| `output_json` | `String` | `output_json` |
| `duration_ms` | `i64` | `duration_ms` |
| `error` | `Option<String>` | `error` |
| `created_at` | `DateTime<Utc>` | `created_at` |

---

### `SyncState` — テーブル `sync_state`

| フィールド | 型 | DB 列 |
|-----------|-----|-------|
| `key` | `String` | `key` PK |
| `value` | `String` | `value` |
| `updated_at` | `DateTime<Utc>` | `updated_at` |

**既定キー:** `gmail_history_id`, `google_tasks_updated_min`, `caldav:{href}:sync-token`

---

## 5. 外部連携 DTO（DB 非永続 or 同期前）

### `CalDavEvent` — 実装済み `src/caldav/event.rs`

CalDAV `calendar-data` パース結果。**DB upsert 前**の一時表現。

```rust
struct CalDavEvent {
    uid: String,
    etag: Option<String>,
    href: Option<String>,
    summary: String,
    starts_at: Option<DateTime<Utc>>,
    ends_at: Option<DateTime<Utc>>,
    all_day: bool,
}
```

### `CalendarCollection` — `src/caldav/xml.rs`

```rust
struct CalendarCollection {
    href: String,
    display_name: Option<String>,
}
```

### `AccessTokenResponse` — `src/google/oauth.rs`

```rust
struct AccessTokenResponse {
    access_token: String,
    expires_in: Option<u64>,
    refresh_token: Option<String>,
    refresh_token_expires_in: Option<u64>,
}
```

### `GmailMessage`（未実装・同期用）

Gmail API から取得し、分類前にメモリ上に保持。

```rust
struct GmailMessage {
    message_id: String,       // Gmail internal id（history 用）
    rfc_message_id: String,   // Message-ID ヘッダ（mail_records.message_id）
    subject: Option<String>,
    sender: Option<String>,
    snippet: String,
    received_at: DateTime<Utc>,
    body_plain: Option<String>, // 分類用。DB には保存しない
}
```

### `GoogleTask`（未実装・同期用）

```rust
struct GoogleTask {
    task_id: String,
    title: String,
    due_at: Option<DateTime<Utc>>,
    completed: bool,
    updated_at: DateTime<Utc>,
}
```

---

## 6. LLM 向け View（rig ツール戻り値）

DB モデルの **部分集合**。LLM コンテキスト用。serde で JSON 化。

### `CalendarEventView`

```json
{
  "id": "uuid",
  "title": "string",
  "starts_at": "RFC3339 | null",
  "ends_at": "RFC3339 | null",
  "state": "scheduled | prepared | completed",
  "remind_enabled": true,
  "all_day": false
}
```

### `TodoView`

```json
{
  "id": "uuid",
  "title": "string",
  "due_at": "RFC3339 | null",
  "state": "todo | ongoing | done",
  "remind_enabled": true
}
```

### `ReminderPlanView`

```json
{
  "id": "uuid",
  "target_type": "calendar_event | todo",
  "target_id": "uuid",
  "scheduled_at": "RFC3339",
  "status": "active | cancelled | completed"
}
```

### `ReminderStatusView`

```json
{
  "id": "uuid",
  "plan_id": "uuid | null",
  "target_type": "calendar_event | todo",
  "target_id": "uuid",
  "target_title": "string",
  "reaction": "no_response | acknowledged",
  "notified_at": "RFC3339",
  "discord_message_id": "string | null"
}
```

`target_*` / `target_title` は JOIN で付与（DB 列ではない）。

### `MailMessageView`（未分類）

```json
{
  "message_id": "string",
  "subject": "string | null",
  "sender": "string | null",
  "snippet": "string",
  "received_at": "RFC3339"
}
```

---

## 7. LLM 出力型（`src/llm/schemas.rs` 想定）

いずれも **`Deserialize` のみ**。プロンプト注入用入力型は別途 `PromptContext` 等で組み立てる。

### `MorningSummaryOutput`

```rust
struct MorningSummaryOutput {
    calendar_lines: Vec<String>,
    todo_lines: Vec<String>,
    mail_lines: Vec<String>,
    #[serde(default)]
    note: Option<String>,
}
```

### `SingleReminderOutput`

```rust
struct SingleReminderOutput {
    title: String,
    body: String,
    urgency: ReminderUrgency,
}
```

### `MailClassifyOutput`

```rust
struct MailClassifyOutput {
    message_id: String,
    category: MailCategory,
    notify_in_summary: bool,
    persist: bool,
}
```

### `IgnoreEvaluateOutput`

```rust
struct IgnoreEvaluateOutput {
    status_id: Uuid,
    judgment: IgnoreJudgment,
    adjustment: IgnoreAdjustment,
    next_scheduled_at: Option<DateTime<Utc>>,
    reason: String,
}
```

### `PlanCreateOutput`

```rust
struct PlanCreateOutput {
    target_type: ReminderTargetType,
    target_id: Uuid,
    scheduled_at: DateTime<Utc>,
    reason: String,
}
```

### `DiscordIntentOutput`

```rust
struct DiscordIntentOutput {
    intent: DiscordIntent,
    target_id: Option<Uuid>,
    payload: serde_json::Value,
}
```

---

## 8. Discord Embed 組み立て用（Rust のみ。LLM 非経由）

### `MorningSummaryEmbed`

| フィールド | ソース |
|-----------|--------|
| `title` | 固定 `"朝サマリー"` + 日付 |
| `calendar_field` | `MorningSummaryOutput.calendar_lines` |
| `todo_field` | `MorningSummaryOutput.todo_lines` |
| `mail_field` | `MorningSummaryOutput.mail_lines` |
| `footer` | `note` |
| `color` | `0x5865F2` |

### `SingleReminderEmbed`

| フィールド | ソース |
|-----------|--------|
| `title` | `SingleReminderOutput.title` |
| `description` | `SingleReminderOutput.body` |
| `color` | urgency から映射 |

---

## 9. 型 ↔ モジュール対応表

| 型 | ファイル（目標） |
|----|------------------|
| 列挙・ドメイン | `src/models/enums.rs`, `src/models/*.rs` |
| View | `src/models/views.rs` |
| LLM schemas | `src/llm/schemas.rs` |
| CalDAV DTO | `src/caldav/event.rs`（現状） |
| Config | `src/config/`（現状） |
