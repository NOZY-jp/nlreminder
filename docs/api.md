# nlreminder API 仕様

関数・rig ツール・LLM 呼び出し・デーモン内部フックの **API 正本**。型定義は [`struct.md`](struct.md)、要件は [`_nlreminder.md`](_nlreminder.md) を参照。

---

## 1. レイヤー構成

```
Discord (serenity) ──► daemon handlers
                            │
                            ├──► llm::complete_json (LM Studio)
                            │         │
                            │         └──► rig tools ──► db / sync / caldav / google
                            │
                            └──► sync jobs (5min scan, 08:00 summary)
```

| レイヤー | 呼び出し元 | LLM から見えるか |
|----------|-----------|-----------------|
| **rig ツール** | LLM（rig 経由） | ○ |
| **llm::\*** | デーモン | × |
| **sync::\*** | デーモン | × |
| **discord::\*** | デーモン | × |
| **caldav / google** | sync, rig, setup CLI | × |
| **CLI** | 人間（`run`, `setup google`） | × |

**原則:** LLM は rig ツールのみ。外部 HTTP API の直接呼び出し禁止。

---

## 2. エラー規約

| 層 | 型 | 振る舞い |
|----|-----|---------|
| ライブラリ内部 | `color_eyre::Result<T>` | `?` で伝播。ログは `tracing` |
| rig ツール | JSON `{ "error": "human-readable message" }` | LLM に返却。`llm_logs` にも記録 |
| LLM JSON パース | 1 回リトライ → 失敗ならスキップ | `llm_logs.error` |
| Google / CalDAV / Gmail 一時障害 | 当該サイクル **スキップ** | 次スキャン（5 分後） |

---

## 3. 実装済みライブラリ API

### `config`

```rust
AppConfig::load() -> Result<AppConfig>
EnvConfig::from_env() -> Result<EnvConfig>
EnvConfig::from_env_for_google_setup() -> Result<EnvConfig>
EnvConfig::google_oauth_config(&self) -> Result<(String, String)>
EnvConfig::google_refresh_token(&self) -> Result<String>
```

### `db`

```rust
connect_and_migrate(database_path: &Path) -> Result<SqlitePool>
```

### `caldav`

```rust
CalDavClient::new(env: &EnvConfig, settings: &Settings) -> Result<CalDavClient>
CalDavClient::list_calendars(&self) -> Result<Vec<CalendarCollection>>
CalDavClient::fetch_events(
    &self,
    range_start: DateTime<Utc>,
    range_end: DateTime<Utc>,
) -> Result<Vec<CalDavEvent>>

default_fetch_range(settings: &Settings) -> Result<(DateTime<Utc>, DateTime<Utc>)>
default_fetch_range_at(
    now: DateTime<Utc>,
    settings: &Settings,
) -> Result<(DateTime<Utc>, DateTime<Utc>)>
```

### `google`

```rust
run_google_setup(env: &EnvConfig) -> Result<()>
check_google_connection(env: &EnvConfig) -> Result<()>
refresh_access_token(env: &EnvConfig) -> Result<AccessTokenResponse>

redirect_uri(port: u16) -> String
authorization_url(client_id: &str, redirect_uri: &str) -> String
```

### `schedule`

```rust
adjust_for_quiet_hours(
    scheduled_at: DateTime<Utc>,
    settings: &Settings,
) -> Result<DateTime<Utc>>

adjust_for_quiet_hours_at(
    scheduled_at: DateTime<Utc>,
    settings: &Settings,
) -> Result<DateTime<Utc>>
```

### `app`

```rust
app::run() -> Result<()>   // デーモン骨格（拡張予定）
```

---

## 4. 未実装ライブラリ API（仕様確定）

### `models` — CRUD

テーブルごとに `src/models/{table}.rs`。引数の ID は `Uuid`。

```rust
// calendar_events
insert(pool, NewCalendarEvent) -> Result<CalendarEvent>
update(pool, CalendarEvent) -> Result<CalendarEvent>
find_by_id(pool, id) -> Result<Option<CalendarEvent>>
find_by_external_uid(pool, uid) -> Result<Option<CalendarEvent>>
list_upcoming(pool, days: i64, settings) -> Result<Vec<CalendarEvent>>

// todos — 同パターン
// mail_records, reminder_plans, reminder_statuses, llm_logs, sync_state — 同パターン
```

`NewCalendarEvent` 等は INSERT 用（`id` / timestamps はリポジトリが生成）。

### `sync::calendar`

```rust
/// CalDAV 取得 → calendar_events upsert。
/// - サーバーに存在: title/starts/ends/etag 更新。nlreminder_owned=false は常にサーバー優先。
/// - 前回 DB にあった external_uid が今回範囲に無い: state=completed。
pub async fn sync_caldav_to_db(
    pool: &SqlitePool,
    client: &CalDavClient,
    settings: &Settings,
) -> Result<SyncCalendarReport>

struct SyncCalendarReport {
    upserted: u32,
    completed: u32,
}
```

取得範囲: `default_fetch_range_at(now, settings)`。

### `sync::todo`

```rust
pub async fn sync_google_tasks_to_db(
    pool: &SqlitePool,
    env: &EnvConfig,
    settings: &Settings,
) -> Result<SyncTodoReport>
```

- リスト: `@default`（`google_tasks_list_id` 空）
- `completed=true` → `state=done`
- `due_at` 変更時 → 連動 CalDAV イベントを upsert（§4.5）

### `sync::mail`

```rust
pub async fn fetch_new_gmail_messages(
    pool: &SqlitePool,
    env: &EnvConfig,
) -> Result<Vec<GmailMessage>>
```

- `sync_state.gmail_history_id` あり → `users.history.list`
- なし → **直近 7 日** `messages.list` で初期化
- 分類・永続化は行わない（LLM + `mail_persist` へ）

### `caldav::write`

```rust
pub async fn put_calendar_event(
    client: &CalDavClient,
    settings: &Settings,
    request: PutCalendarEventRequest,
) -> Result<PutCalendarEventResponse>

struct PutCalendarEventRequest {
    uid: String,              // 新規なら UUID、Todo 連動なら nlreminder-todo-{id}
    title: String,
    starts_at: DateTime<Utc>,
    ends_at: DateTime<Utc>,
    all_day: bool,
    etag: Option<String>,     // 更新時必須（If-Match）
    href: Option<String>,     // 更新時
    nlreminder_owned: bool,
}

struct PutCalendarEventResponse {
    href: String,
    etag: String,
    uid: String,
}
```

**Todo 締切連動:** タイトル `TODO: {title}`、終日、`uid = nlreminder-todo-{todo_id}`。

### `sync::todo_calendar`

```rust
pub async fn sync_todo_deadline_to_caldav(
    pool: &SqlitePool,
    client: &CalDavClient,
    settings: &Settings,
    todo: &Todo,
) -> Result<()>
```

`due_at` が `None` なら CalDAV 側イベントは作らない（既存があれば completed 扱いは **しない**。due 削除は手動運用）。

### `llm`

```rust
pub async fn complete_json<T: DeserializeOwned>(
    pool: &SqlitePool,
    env: &EnvConfig,
    call_type: LlmCallType,
    system: &str,
    user: &str,
) -> Result<T>
```

- POST `{LMSTUDIO_BASE_URL}/chat/completions`
- `model = env.lmstudio_model`
- `response_format: { type: "json_object" }`
- 成功 / 失敗を `llm_logs` に INSERT
- パース失敗時 **1 回** 同一プロンプトでリトライ

### `discord`

```rust
pub async fn start_bot(config: AppConfig, pool: SqlitePool) -> Result<()>

pub async fn send_morning_summary(channel_id, embed: MorningSummaryEmbed) -> Result<MessageId>
pub async fn send_single_reminder(channel_id, embed: SingleReminderEmbed) -> Result<MessageId>

/// 通知直後に reminder_statuses へ no_response で INSERT
pub async fn record_notification(
    pool: &SqlitePool,
    plan_id: Option<Uuid>,
    discord_message_id: &str,
) -> Result<Uuid>
```

**メッセージ受信:** 通知チャンネル内の **ユーザー投稿のみ** 処理 → `discord_intent` LLM 呼び出し。

### `daemon` スケジューラ（`app/daemon.rs` 内）

| タスク | 間隔 / トリガ | 処理 |
|--------|--------------|------|
| `scan_loop` | `scan_interval_secs` | sync ×3 → 新規検知 → `plan_create` LLM → heartbeat |
| `morning_summary_loop` | 毎日 `morning_summary_hour`（ローカル） | 集約 → `morning_summary` LLM → Discord 1 通 |
| `reminder_due_loop` | `scan_loop` 内または 1 分 | `reminder_plan_list_due` → 随時リマインド |
| `backup_loop` | 日曜 04:00 JST | DB ファイルコピー、4 世代削除 |

---

## 5. rig ツール API

登録名 = JSON の tool name。引数・戻り値は **camelCase なし（snake_case）**。

共通エラー:

```json
{ "error": "calendar event not found: {id}" }
```

### 5.1 `calendar_list_upcoming`

**入力**

```json
{ "days": 7 }
```

`days` 省略時は `7`。

**出力**

```json
{
  "events": [ /* CalendarEventView[] */ ]
}
```

### 5.2 `calendar_get`

**入力** `{ "id": "uuid" }`  
**出力** `{ "event": CalendarEventView }`

### 5.3 `calendar_create`

**入力**

```json
{
  "title": "string",
  "starts_at": "RFC3339",
  "ends_at": "RFC3339",
  "all_day": false
}
```

**出力** `{ "id": "uuid", "external_uid": "string" }`

副作用: CalDAV PUT（`sshCalendar`）+ DB insert。`nlreminder_owned = true`。

### 5.4 `calendar_update`

**入力**

```json
{
  "id": "uuid",
  "title": "string?",
  "starts_at": "RFC3339?",
  "ends_at": "RFC3339?",
  "state": "scheduled | prepared | completed?"
}
```

**出力** `{ "event": CalendarEventView }`

- `nlreminder_owned = false` のイベント: **`state` / `remind_enabled` の DB 更新のみ**（CalDAV の title/日時は変更しない）
- `nlreminder_owned = true`: CalDAV + DB 更新

### 5.5 `calendar_set_remind_enabled`

**入力** `{ "id": "uuid", "enabled": true }`  
**出力** `{ "ok": true }`

### 5.6 `todo_list_open`

**入力** `{}`  
**出力** `{ "todos": [ TodoView ] }` — `state != done`

### 5.7 `todo_create`

**入力** `{ "title": "string", "due_at": "RFC3339?" }`  
**出力** `{ "id": "uuid" }`

副作用: Google Tasks API insert + DB insert + `due_at` あれば `sync_todo_deadline_to_caldav`。

### 5.8 `todo_update`

**入力**

```json
{
  "id": "uuid",
  "title": "string?",
  "due_at": "RFC3339?",
  "state": "todo | ongoing | done?"
}
```

**出力** `{ "todo": TodoView }`

`state = done` → Google Tasks 完了 API。

### 5.9 `todo_set_remind_enabled`

**入力** `{ "id": "uuid", "enabled": true }`  
**出力** `{ "ok": true }`

### 5.10 `mail_list_unclassified`

**入力** `{ "limit": 20 }`  
**出力** `{ "messages": [ MailMessageView ] }`

メモリ / 一時キュー上の未分類メール（`mail_records` 未登録かつ今サイクル未取得分）。

### 5.11 `mail_persist`

**入力**

```json
{
  "message_id": "string",
  "subject": "string?",
  "sender": "string?",
  "received_at": "RFC3339"
}
```

**出力** `{ "id": "uuid" }`

### 5.12 `reminder_plan_get_active`

**入力** `{ "target_type": "calendar_event | todo", "target_id": "uuid" }`  
**出力** `{ "plan": ReminderPlanView | null }`

### 5.13 `reminder_plan_create`

**入力**

```json
{
  "target_type": "calendar_event | todo",
  "target_id": "uuid",
  "scheduled_at": "RFC3339",
  "reason": "string?"
}
```

**出力** `{ "plan": ReminderPlanView }`

副作用:

1. 同一 `(target_type, target_id)` の active を `cancelled`
2. `scheduled_at` = `adjust_for_quiet_hours(scheduled_at)`
3. INSERT status=`active`

### 5.14 `reminder_plan_cancel`

**入力** `{ "id": "uuid" }`  
**出力** `{ "ok": true }`

### 5.15 `reminder_plan_list_due`

**入力** `{ "before": "RFC3339" }`  
**出力** `{ "plans": [ ReminderPlanView ] }`

`status = active` かつ `scheduled_at <= before`。

### 5.16 `reminder_status_list_no_response`

**入力** `{ "since_hours": 24 }`  
**出力** `{ "statuses": [ ReminderStatusView ] }`

### 5.17 `reminder_status_acknowledge`

**入力** `{ "id": "uuid" }`  
**出力** `{ "ok": true }`

`reaction = acknowledged`, `acknowledged_at = now`。

---

## 6. LLM 呼び出し API（デーモン内部）

rig ツールではない。`llm::complete_json<T>` の `T` とプロンプト役割。

| call_type | 出力型 `T` | トリガ |
|-----------|-----------|--------|
| `morning_summary` | `MorningSummaryOutput` | 08:00 |
| `single_reminder` | `SingleReminderOutput` | 計画 due |
| `mail_classify` | `MailClassifyOutput` | scan 内（メール 1 件ずつ） |
| `ignore_evaluate` | `IgnoreEvaluateOutput` | heartbeat（候補 1 件ずつ） |
| `plan_create` | `PlanCreateOutput` | 新規予定/Todo 検知 |
| `discord_intent` | `DiscordIntentOutput` | ユーザー投稿 |

### `discord_intent` プロンプト注入（システム側）

Reply あり:

```json
{
  "reply_to_status_id": "uuid",
  "recent_open_statuses": [ /* ReminderStatusView, max 5 */ ]
}
```

Reply なし: 直近 24h の `no_response` 一覧（最大 10 件）を同様に注入。

### `mail_classify` 後処理（デーモン）

```
LLM → MailClassifyOutput
  if persist → mail_persist tool 相当の内部呼び出し
  notify_in_summary → 朝サマリー用インメモリフラグ
```

---

## 7. CLI（人間向け）

| コマンド | API |
|----------|-----|
| `cargo run -- run` | `app::run()` |
| `cargo run -- setup google` | `google::run_google_setup` |
| `cargo run -- setup google --check` | `google::check_google_connection` |

---

## 8. 外部 HTTP API（参考。LLM 禁止）

実装は `google` / `caldav` / `llm` モジュール内に封じる。

| サービス | エンドポイント例 |
|----------|-----------------|
| Google OAuth | `oauth2.googleapis.com/token` |
| Gmail | `gmail.googleapis.com/gmail/v1/users/me/...` |
| Tasks | `tasks.googleapis.com/tasks/v1/lists/@default/tasks` |
| CalDAV | PROPFIND / REPORT / PUT on `sshCalendar` |
| LM Studio | `POST /v1/chat/completions` |

---

## 9. heartbeat アルゴリズム（疑似コード）

```
on_heartbeat():
  statuses = reminder_status_list_no_response(since_hours=24)
  for s in statuses:
    if s.reaction == acknowledged: continue
    out = llm.complete_json::<IgnoreEvaluateOutput>(ignore_evaluate, context(s))
    if out.adjustment in (delay, intensify):
      reminder_plan_create(..., out.next_scheduled_at)
    else if out.adjustment == keep:
      pass  // 据え置き
```

トリガ: リマインド実行後 / Discord 通知後 / 定期スキャン開始時。

---

## 10. 関連ドキュメント

| ファイル | 内容 |
|----------|------|
| [`struct.md`](struct.md) | 型・DB 列 |
| [`dev.md`](dev.md) | TDD・コミット規約 |
| [`_nlreminder.md`](_nlreminder.md) §9 | 要約（本ファイルが詳細版） |
