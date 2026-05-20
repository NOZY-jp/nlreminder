# nlreminder 初回セットアップ手順

Google Todo・Gmail 連携に必要な OAuth 設定と、`.env` への反映方法。

**本番運用を前提とする。** Testing モードの refresh token は 7 日で失効するため、Playground は使わず `nlreminder setup google` で取得する。

---

## 前提

- Google アカウントを 1 つ決めておく（Todo・Gmail 両方で使う）
- OAuth クライアントは **Web アプリケーション** 型
- 同意画面は **本番（In production）** に公開する

---

## 1. Google Cloud プロジェクトの作成

1. [Google Cloud Console](https://console.cloud.google.com/) を開く
2. 画面上部のプロジェクト選択 → **新しいプロジェクト**
3. プロジェクト名（例: `nlreminder`）を入力して作成
4. 作成したプロジェクトを選択した状態にする

---

## 2. API の有効化

1. **API とサービス** → **ライブラリ**
2. 次の API をそれぞれ検索し、**有効にする**
   - **Gmail API**
   - **Google Tasks API**

---

## 3. OAuth 同意画面の設定

1. **API とサービス** → **OAuth 同意画面**
2. ユーザータイプ: **外部**（個人 Google アカウントの場合）
3. アプリ名（例: `nlreminder`）、ユーザーサポートメール、デベロッパーの連絡先メールを入力
4. **スコープの追加** で以下を追加:
   - `https://www.googleapis.com/auth/gmail.readonly`
   - `https://www.googleapis.com/auth/tasks`
5. 保存

### 本番公開（必須）

1. 同じ **OAuth 同意画面** で **アプリを公開**（Publish app）を実行
2. 確認ダイアログで **確認** を選択

> **Testing** のままだと refresh token は **7 日で失効** する（`refresh_token_expires_in: 604799` が付く）。本番運用では必ず **In production** にする。

Gmail / Tasks は sensitive スコープのため、未確認アプリとして警告画面が出ることがある。自分だけが使う個人ツールであれば **続行** してよい。

---

## 4. OAuth クライアント ID の作成

1. **API とサービス** → **認証情報**
2. **認証情報を作成** → **OAuth クライアント ID**
3. アプリケーションの種類: **Web アプリケーション**
4. 名前（例: `nlreminder-oauth`）を入力
5. **承認済みのリダイレクト URI** に以下を追加:
   ```
   http://127.0.0.1:8080/oauth/callback
   ```
6. 作成し、**クライアント ID** と **クライアント シークレット** を控える

---

## 5. `.env` の初期設定

`.env.example` をコピーして `.env` を作成し、最低限以下を埋める。

```env
GOOGLE_CLIENT_ID=your-client-id.apps.googleusercontent.com
GOOGLE_CLIENT_SECRET=your-client-secret
GOOGLE_ACCOUNT_EMAIL=your@gmail.com
```

`GOOGLE_REFRESH_TOKEN` は次の手順で取得する。

---

## 6. リフレッシュトークンの取得（本番フロー）

プロジェクトルートで:

```bash
cargo run -- setup google
```

1. ターミナルに表示される URL をブラウザで開く
2. 連携する Google アカウントでログイン・許可
3. ブラウザが `127.0.0.1` にリダイレクトされ、ターミナルに refresh token が表示される
4. 表示された行を `.env` の `GOOGLE_REFRESH_TOKEN=` に貼り付ける

本番公開済みであれば、レスポンスに `refresh_token_expires_in` が **付かない**（または非常に長い）のが正常。

### 動作確認

```bash
cargo run -- setup google --check
```

アクセストークンを refresh し、Gmail プロフィール取得で連携を確認する。

---

## 7. その他の設定

`.env.example` と `config.toml.example` を参照し、残りを設定する。

| 種別 | ファイル | 例 |
|------|----------|-----|
| シークレット | `.env` | Discord Token, CalDAV パスワード |
| 動作設定 | `config.toml` | 朝 8:00、スキャン間隔、DB パス |

```bash
cp config.toml.example config.toml
```

---

## 8. 全体の動作確認

1. LM Studio で **Qwen3.6 35B** をロードし、Local Server を起動（`http://localhost:1234`）
2. `cargo run -- setup google --check` が成功する
3. `cargo run -- run` でデーモンが起動する（実装進行に応じて機能が増える）

---

## トラブルシューティング

| 症状 | 対処 |
|------|------|
| `redirect_uri_mismatch` | Cloud Console のリダイレクト URI が `http://127.0.0.1:8080/oauth/callback` と完全一致しているか確認 |
| `refresh_token_expires_in: 604799` | 同意画面が **Testing** のまま。本番公開後に `setup google` を再実行 |
| `invalid_grant` | refresh token 再取得。`setup google` を再実行（`prompt=consent` で新規発行） |
| `access_denied` | 本番公開後も sensitive スコープは警告が出る。**Advanced → Go to nlreminder (unsafe)** で続行 |
| refresh token が表示されない | 以前認可済みの場合がある。Google アカウント → セキュリティ → サードパーティアクセスから nlreminder を削除して再実行 |
| `insufficientPermissions` | スコープに `gmail.readonly` と `tasks` が含まれているか確認 |
| ポート 8080 が使用中 | 他プロセスを停止するか、実装側の `OAUTH_CALLBACK_PORT` 環境変数で変更（要 Cloud Console URI 追加） |

---

## 補足: OAuth 2.0 Playground について

開発中の暫定手段として Playground も使えるが、**Testing モードでは 7 日で失効** する。本番運用では **使わない**。
