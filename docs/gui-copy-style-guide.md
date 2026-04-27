# VAC GUI 文体・用語ガイド

> Status: GUI copy cleanup の基準案。VAC の GUI / docs / source comments は日本語を基準文体としつつ、技術語・識別子・既存の英語 UI 慣習は自然に併用する。

---

## 1. 基本方針

VAC の主要開発者と想定ユーザーは日本語ネイティブであり、English は第2言語として扱う。したがって、ユーザーが読む説明文、警告、確認、状態表示、source comment は原則として日本語を基準にする。

ただし、英語を排除しない。日本語の中に English spell、カタカナ語、API 名、ライブラリ名、一般的な UI 用語が混ざる状態を自然な表現として扱う。無理な日本語化で意味が曖昧になる場合や、設定ファイル・ログ・API と照合しづらくなる場合は英語のまま残す。

GUI copy は、次の3つを同時に満たすことを目標にする。

- わかりやすい: 今の状態、操作結果、危険な影響がすぐ読める。
- 使いやすい: ボタン名、見出し、確認文がユーザーの作業手順と一致している。
- さり気なくかっこいい: 常駐ランタイムの管制卓として、軽く引き締まった雰囲気を持つ。

「かっこよさ」は装飾的な言い回しで盛ることではない。短く、具体的で、機械と協調している感覚があることを重視する。

---

## 2. 表示領域ごとの言語基準

| 領域 | 基準 | 例 |
| --- | --- | --- |
| 見出し / タブ / ボタン | 短い日本語または定着済み英語 | `接続`, `連携アプリ`, `再起動`, `Flowgraph Studio` |
| 説明文 / empty state / helper text | 日本語の文章 | `run_with に登録された Managed App を起動・停止できます。` |
| 確認ダイアログ / Toast | 日本語。主語と結果を短く | `VAC を終了しますか？`, `終了を要求しました` |
| エラー詳細 | 日本語の見出し + backend detail はそのまま | `起動に失敗しました`, `409 Conflict` |
| 技術識別子 / API / config key | 原則そのまま | `run_with`, `if_not_running`, `Control API`, `PID` |
| 製品内コンセプト名 | 初出や見出しは英語名を維持可 | `Runtime Mode`, `Managed App`, `Flowgraph Studio` |
| source comment | 原則日本語。英語 API 名はそのまま | `WebSocket イベントを反映する` |
| test name / spec title | 日本語でも英語でも可。UI文言期待値は実表示に合わせる | `連携アプリドロワーを開ける` |

---

## 3. 残す英語

以下は原則として英語のまま使う。

- 固有名: `Virtual Avatar Connect`, `VAC`, `Flowgraph Studio`
- Runtime 概念: `Runtime Mode`, `Managed App`, `Control API`, `WebSocket`
- config / API / protocol identifier: `run_with`, `if_not_running`, `profile`, `PID`, `OAuth`, `Twitch`
- 開発者向けの短いラベルで英語の方が明確なもの: `Resources`, `Settings`, `Observability`
- backend から返る `kind`, `status`, `severity`, `code` などの machine-readable value

ただし、周辺文は日本語にする。例: `Managed App status from the current run_with registry.` ではなく、`現在の run_with 登録から Managed App の状態を確認します。`

---

## 4. 言い換える日本語

英語化された UI copy は、次の粒度で戻す。

| 英語 | 推奨日本語 | 備考 |
| --- | --- | --- |
| `Connection` | `接続` | 状態カード・設定見出し |
| `Managed Apps` | `連携アプリ` | 表示名。概念説明では `Managed App` も併用 |
| `Resource Overview` | `リソース概要` | Resources タブ内 |
| `Refresh` | `更新` | ボタン |
| `Restart...` | `再起動...` | 破壊的ではないが接続断あり |
| `Shutdown` | `終了` | 確認文で影響を補足 |
| `Start` / `Stop` / `Restart` / `Minimize` | `起動` / `停止` / `再起動` / `最小化` | Managed App 操作 |
| `Loading...` | `読み込み中...` | 汎用 |
| `running` / `stopped` | `起動中` / `停止中` | UI表示。イベント生値は必要に応じて保持 |
| `No ... registered.` | `... は未登録です。` | empty state |
| `planned` | `予定` | placeholder subtitle |

---

## 5. 文体

- 丁寧語は使いすぎないが、GUI 表示はぶっきらぼうにしない。
- 文末は `です / ます` を基本にし、確認ダイアログは `するか？` より `しますか？` を優先する。
- Toast は短い完了形にする。例: `終了を要求しました`, `起動に失敗しました`
- エラーや危険操作では、何が起きるかを1文で明示する。
- カタカナ語と英語は、読者が設定ファイルやログと照合しやすい場合に残す。
- 同じ対象には同じ表現を使う。`連携アプリ` と `Managed App` を混ぜる場合は、表示名が `連携アプリ`、概念名が `Managed App` という役割を守る。
- ユーザー向け説明は「何ができるか」より「今何が起きているか / 何を変更するか」を優先する。
- 状態表示はやや硬質でよい。例: `稼働中`, `要確認`, `停止中`, `追跡対象`
- 説明文は落ち着いた実務文にする。例: `現在の run_with 登録から Managed App の状態を確認します。`
- 煽り文句、過剰な親しみ、マーケティング風の文言は使わない。
- かっこよさを出すために、英語を増やす必要はない。必要な技術語を自然に残し、余計な説明を削る。

---

## 6. 編集手順

1. copy-only の英語化は revert を優先する。
2. 機能追加を含む変更は、機能差分を残して文言だけ本ガイドへ合わせる。
3. Playwright の期待値は、実表示の日本語 copy に合わせて更新する。
4. ソースコメントは、触るファイルから順に日本語基準へ揃える。無関係ファイルの大規模置換は避ける。
5. 英語のまま残す判断をした場合は、固有名・識別子・開発者向け語彙のどれに該当するかを意識する。
