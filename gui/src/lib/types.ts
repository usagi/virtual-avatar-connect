/**
 * VAC Control API の DTO を TypeScript に写したもの。
 *
 * **本ファイルは Rust 側の真実の写し** であり、以下のファイルと 1:1 で整合させること:
 *   - src/web_interface/control/ping.rs       → PingResponse / WhoAmIResponse
 *   - src/web_interface/control/dto.rs        → StateSnapshot / *Summary
 *   - src/web_interface/control/actions.rs    → PauseTarget / PauseOutcome
 *   - src/web_interface/control/reload.rs     → ReloadRequest / ReloadResponse
 *   - src/twitch_oauth_sessions.rs（Control API は oauth_twitch.rs）→ OAuth* 一式
 *   - src/control_events.rs                   → ControlEvent (discriminated union)
 *   - src/ai/reload.rs                        → AiReloadRequest / AiReloadReport
 *
 * 受信側（WS の ControlEvent）は必ず `switch (ev.kind)` で分岐し、
 * 末尾に `const _: never = ev;` を置いて exhaustive check する。Rust 側で variant を増やしたとき
 * TypeScript 側のハンドラ漏れが **compile-time に** 検出される設計。
 *
 * I/O 境界での型擬態（`fetch().json()` が `any` を返す）は承知のうえで、
 * ここでは型を "信頼" する選択をしている。将来 valibot/zod での runtime validation を挟む予定。
 */

// ---------------------------------------------------------------------------
// /ping, /whoami
// ---------------------------------------------------------------------------

export type PingResponse = {
 ok: boolean;
 service: string;
 version: string;
 now: string;
};

export type WhoAmIResponse = {
 peer_addr: string | null;
 is_loopback: boolean;
 required_token: boolean;
 token_source: 'env' | 'config' | 'generated';
 token_file: string | null;
};

// ---------------------------------------------------------------------------
// /snapshot
// ---------------------------------------------------------------------------

export type StateSnapshot = {
 schema: number;
 app_version: string;
 now: string;
 runtime: RuntimeSummary;
 processors: ProcessorSummary[];
 ai_personas: AiPersonaSummary[];
 twitch: TwitchSummary | null;
};

export type RuntimeSummary = {
 session_id: string;
 root: string;
 session_dir: string;
 inline_max_bytes: number;
};

export type ProcessorSummary = {
 index: number;
 feature: string;
 id: string | null;
 paused: boolean;
 last_invoked_at: string | null;
 /** Phase VI-γ-8a: `feature === "modify"` のときだけ present。`writable_dictionary_file` が設定されている processor のみ値が入る。 */
 writable_dictionary_file?: string;
 writable_regex_file?: string;
};

export type AiPersonaSummary = {
 index: number;
 id: string | null;
 paused: boolean;
};

export type TwitchSummary = {
 username: string;
 channel_to: string;
 reads: string[] | null;
 eventsub_enabled: boolean;
 moderator_enabled: boolean;
 moderator_login: string | null;
 ignore_logins: string[];
 /** broadcaster 用の保存トークンが有効か。ident が組めないときは false。 */
 broadcaster_authorized: boolean;
 /**
  * moderator 用の保存トークンが有効か。
  * - `null`: `[twitch.moderator]` 設定そのものが無い
  * - `false`: 設定はあるが有効なトークンが無い／期限切れ
  * - `true`: 認可済み
  */
 moderator_authorized: boolean | null;
};

// ---------------------------------------------------------------------------
// /pause, /resume
// ---------------------------------------------------------------------------

export type PauseTarget =
 | { target: 'all' }
 | { target: 'processors' }
 | { target: 'ais' }
 | { target: 'processor'; id?: string | null; index?: number | null }
 | { target: 'ai'; id?: string | null; index?: number | null };

export type PauseOutcome = {
 processors_affected: number[];
 ais_affected: number[];
 paused: boolean;
};

// ---------------------------------------------------------------------------
// /reload
// ---------------------------------------------------------------------------

export type AiReloadRequest = {
 /** `null`: 変更しない / `""`: instructions クリア / `"xxx"`: 差し替え */
 custom_instructions?: string | null;
 system_instructions_extra?: string | null;
 heartbeat_enabled?: boolean | null;
 decision_threshold?: number | null;
};

export type AiReloadReport = {
 custom_instructions_changed: boolean;
 system_instructions_extra_changed: boolean;
 heartbeat_enabled_changed: boolean;
 decision_threshold_changed: boolean;
 rebuilt_request_template: boolean;
 rebuilt_decision_spec: boolean;
 warnings: string[];
};

export type ReloadRequest =
 | ({ target: 'ai_persona'; id?: string | null } & AiReloadRequest)
 | { target: 'modify_files'; id?: string | null };

export type ModifyReport = {
 processor_index: number;
 processor_id: string | null;
 feature: string;
 dictionary_entries: number;
 regex_entries: number;
};

export type ReloadResponse = {
 target: 'ai_persona' | 'modify_files';
 ai_report?: AiReloadReport;
 modify_reports?: ModifyReport[];
};

// ---------------------------------------------------------------------------
// /oauth/twitch/*
// ---------------------------------------------------------------------------

export type OAuthAccount = 'broadcaster' | 'moderator';

export type OAuthSessionStatus = 'pending' | 'authorized' | 'expired' | 'canceled' | 'failed';

export type OAuthSessionView = {
 account: OAuthAccount;
 status: OAuthSessionStatus;
 user_code: string;
 verification_uri: string;
 interval_secs: number;
 expires_at: string;
 started_at: string;
 last_error?: string;
};

export type OAuthStartResponse = {
 outcome: 'started' | 'already_pending' | 'already_authorized';
 session: OAuthSessionView | null;
};

export type OAuthCancelResponse = {
 canceled: boolean;
 session: OAuthSessionView;
};

/**
 * `DELETE /oauth/twitch/{account}/tokens` のレスポンス。
 *   - `deleted`: 実際にファイルが削除されたか（元々無ければ false）
 *   - `path`: 削除されたファイルのパス（存在しなければ null）
 *   - `session_removed`: 既存のセッションが除去されたか
 */
export type OAuthDeleteTokensResponse = {
 account: OAuthAccount;
 deleted: boolean;
 path: string | null;
 session_removed: boolean;
};

// ---------------------------------------------------------------------------
// /ingress
// ---------------------------------------------------------------------------

/** ChannelDatum.source に載せる由来情報。 */
export type DataSource = {
 /** 例: `"control.ingress"`, `"twitch.eventsub"`, `"voice.whisper"`。 */
 kind: string;
 subtype?: string | null;
 /** 表示用の当事者名（ユーザー名、発言者名など）。 */
 actor?: string | null;
};

/**
 * `POST /api/v1/control/ingress` のリクエスト。
 * `channel` だけが必須で、他はすべて省略可能。
 */
export type IngressRequest = {
 channel: string;
 content: string;
 /** 省略時 backend 側で `true` 扱い。 */
 is_final?: boolean;
 /** 追加で立てたい flags。`is_final` 以外を想定。 */
 flags?: string[];
 /** 未指定時は backend が `{ kind: "control.ingress", actor: "gui" }` を補う。 */
 source?: DataSource;
 /** 自由形式の構造化メタ。JSON object。 */
 meta?: Record<string, unknown>;
};

export type IngressResponse = {
 id: number;
 channel: string;
 accepted: boolean;
};

// ---------------------------------------------------------------------------
// /restart, /profiles (Phase VI-γ-1)
// ---------------------------------------------------------------------------

export type RestartRequest = {
 /** 現 conf と同ディレクトリの相対パスまたは絶対パス。未指定時は現 conf を引き継いで再起動。 */
 conf?: string;
 /** 再起動前の猶予 ms（default 800, max 10000）。 */
 graceful_ms?: number;
};

export type RestartResponse = {
 new_conf_path: string;
 new_pid: number;
 graceful_ms: number;
 current_pid: number;
};

// ---------------------------------------------------------------------------
// /shutdown (Phase ε-1)
// ---------------------------------------------------------------------------

export type ShutdownRequest = {
 /** 予約済み。cleanup の待ち時間ヒント。現状はサーバ側で無視される。 */
 graceful_ms?: number;
};

export type ShutdownResponse = {
 /** 常に `"shutting_down"`。冪等に何度叩いても同じ値が返る。 */
 status: string;
 /** 停止要求を受け付けたプロセスの PID。 */
 current_pid: number;
};

export type ProfileEntry = {
 path: string;
 filename: string;
 label: string;
 size: number;
 modified: string | null;
 is_current: boolean;
};

export type ProfilesResponse = {
 directory: string | null;
 entries: ProfileEntry[];
 current: string | null;
};

// ---------------------------------------------------------------------------
// /profiles/* operations (Phase VI-γ-4a)
// ---------------------------------------------------------------------------

export type ProfileContentResponse = {
 filename: string;
 path: string;
 size: number;
 content: string;
 is_current: boolean;
};

// ---------------------------------------------------------------------------
// /processors & /ai_personas PUT (Phase VI-γ-3b)
// ---------------------------------------------------------------------------

export type NodePutConfigRequest = {
 config: JsonValue;
};

export type NodePutConfigResponse = {
 index: number;
 source_path: string;
 backup: string;
 warning: string;
};

export type ProfileOpResponse = {
 filename: string;
 path: string;
 /** 操作時に生成されたバックアップファイル名（あれば）。 */
 backup: string | null;
 /** 付随する警告（例: 現 conf を更新したので再起動が必要）。 */
 warning?: string;
};

export type ProfileCloneRequest = {
 source: string;
 new_filename: string;
};

export type ProfileRenameRequest = {
 new_filename: string;
};

export type ProfilePutContentRequest = {
 content: string;
};

// ---------------------------------------------------------------------------
// /run_with (Phase VI-γ-5a)
// ---------------------------------------------------------------------------

export type RunWithTableDto = {
 command: string;
 if_not_running?: string | null;
 run_as_admin?: boolean | null;
 working_dir?: string | null;
 minimized?: boolean | null;
 id?: string | null;
 label?: string | null;
};

/** 文字列 (`"notepad"`) or table 形式。JSON 上は untagged。 */
export type RunWithDto = string | RunWithTableDto;

export type RunWithView = {
 index: number;
 effective_id: string;
 display_label: string;
 supports_status: boolean;
 entry: RunWithDto;
};

export type RunWithListResponse = {
 entries: RunWithView[];
 source_path: string | null;
};

export type RunWithMutationResponse = {
 entries: RunWithView[];
 source_path: string | null;
 backup: string | null;
 warning?: string | null;
};

// ---------------------------------------------------------------------------
// /bos (Phase VI-γ-2a)
// ---------------------------------------------------------------------------

export type BosCategory = 'subtitles' | 'bgm' | 'effects' | 'other';

export type BosEntry = {
 id: string;
 title: string;
 category: BosCategory;
 /** `/browser-output/<id>/` 等の相対 URL。GUI 側で origin と合成。 */
 url: string;
 path: string;
 supports_channel_param: boolean;
 user_defined: boolean;
};

export type BosResponse = {
 document_root: string | null;
 url_prefix: string;
 entries: BosEntry[];
};

// ---------------------------------------------------------------------------
// /managed_apps (Phase VI-γ-2b)
// ---------------------------------------------------------------------------

export type ManagedAppStatus = {
 id: string;
 running: boolean;
 pids: number[];
 /** RFC3339 datetime */
 checked_at: string;
};

export type ManagedAppView = {
 id: string;
 label: string;
 command: string;
 process_marker: string | null;
 minimized: boolean;
 run_as_admin: boolean;
 working_dir: string | null;
 supports_status: boolean;
 status: ManagedAppStatus;
};

export type ManagedAppsResponse = {
 entries: ManagedAppView[];
};

export type ManagedAppStopRequest = {
 grace_ms?: number;
};

export type ManagedAppStopResponse = {
 id: string;
 closed_windows: number;
 terminated_pids: number;
 note?: string;
};

export type ManagedAppStartResponse = {
 id: string;
 was_running: boolean;
};

export type ManagedAppMinimizeResponse = {
 id: string;
 scheduled_pids: number;
};

export type ManagedAppRestartResponse = {
 id: string;
 closed_windows: number;
 terminated_pids: number;
 was_running: boolean;
};

// ---------------------------------------------------------------------------
// /modify/:id/dictionary|regex/entries (Phase VI-γ-8a)
// ---------------------------------------------------------------------------

export type DictionaryEntryRequest = {
 to: string;
 from: string;
};

export type RegexEntryRequest = {
 replacement: string;
 pattern: string;
};

export type DictionaryEntryResponse = {
 processor_index: number;
 processor_id: string;
 file: string;
 total_entries: number;
 already_present: boolean;
 removed_lines: number;
};

export type RegexEntryResponse = DictionaryEntryResponse;

/**
 * 外部エディタ委譲（γ-9）レスポンス。
 * サーバ側で `cmd /C start` / `open` / `xdg-open` を fire-and-forget で起動した結果を返す。
 */
export type OpenExternalResponse = {
 processor_index: number;
 processor_id: string;
 kind: 'dictionary' | 'regex';
 file: string;
 command: string;
 spawned: boolean;
};

// ---------------------------------------------------------------------------
// Modify content read-only views (Phase VI-γ-8d)
// ---------------------------------------------------------------------------

/** 辞書 1 行ぶん（`to<space>from`）。 */
export type DictionaryRow = { to: string; from: string };

export type DictionaryFileView = {
 file: string;
 is_writable: boolean;
 entries: DictionaryRow[];
 error?: string | null;
};

export type DictionaryContentResponse = {
 processor_index: number;
 processor_id: string;
 writable_file: string | null;
 files: DictionaryFileView[];
};

/** 正規表現 1 行ぶん。`valid: false` のときはメモリにロードされていない（= 置換対象から外れている）。 */
export type RegexRow = {
 replacement: string;
 pattern: string;
 valid: boolean;
 parse_error?: string | null;
};

export type RegexFileView = {
 file: string;
 is_writable: boolean;
 is_csv: boolean;
 entries: RegexRow[];
 error?: string | null;
};

export type RegexContentResponse = {
 processor_index: number;
 processor_id: string;
 writable_file: string | null;
 files: RegexFileView[];
};

// ---------------------------------------------------------------------------
// /processors/:index/config, /ai_personas/:index/config (Phase VI-γ-3a)
// ---------------------------------------------------------------------------

/** 任意の JSON 値。ProcessorConf / AiPersonaConf は巨大な構造体で、GUI 側では JSON ビューに生で流す。 */
export type JsonValue = null | boolean | number | string | JsonValue[] | { [k: string]: JsonValue };

export type ProcessorConfigResponse = {
 index: number;
 feature: string | null;
 id: string | null;
 config: JsonValue;
 source_path: string | null;
};

export type AiPersonaConfigResponse = {
 index: number;
 id: string | null;
 config: JsonValue;
 source_path: string | null;
};

// ---------------------------------------------------------------------------
// /events — WebSocket の discriminated union
// ---------------------------------------------------------------------------

export type ChannelDatumPhase = 'pushed' | 'pushed_quiet' | 'updated' | 'finalized';

/** `ProcessorInvoked.outcome` の種別。pulse の色分けに使う。 */
export type ProcessorInvocationOutcome = 'continued' | 'break' | 'error';

/** Rust の `ControlEvent` enum と完全対応。`kind` discriminator で分岐する。 */
export type ControlEvent =
 | {
    kind: 'channel_datum';
    phase: ChannelDatumPhase;
    id: number;
    channel: string;
    content: string;
    flags: string[];
    datetime: string;
   }
 | {
    kind: 'lagged';
    dropped: number;
   }
 | {
    kind: 'heartbeat';
    now: string;
   }
 | {
    kind: 'pause_state';
    paused: boolean;
    target: 'all' | 'processors' | 'ais' | 'processor' | 'ai';
    processors_affected: number[];
    ais_affected: number[];
   }
 | {
    kind: 'reloaded';
    target: 'ai_persona' | 'modify_files';
    id: string | null;
    detail: unknown; // AiReloadReport | ModifyReport[] だが実装側で narrow する
   }
 | {
    kind: 'oauth_status';
    account: OAuthAccount;
    status: OAuthSessionStatus;
    view: OAuthSessionView;
   }
 | {
    kind: 'processor_invoked';
    index: number;
    feature: string;
    id: string | null;
    channel_datum_id: number;
    trigger_channel: string;
    elapsed_ms: number;
    outcome: ProcessorInvocationOutcome;
   }
 | {
    kind: 'restarting';
    new_conf_path: string;
    new_pid: number;
    graceful_ms: number;
    current_pid: number;
   }
 | {
    kind: 'managed_app_state';
    id: string;
    running: boolean;
    pids: number[];
    /** RFC3339 datetime */
    checked_at: string;
   }
 | {
    /** Phase δ-6: Flowgraph ロード結果の更新通知。GUI は diagnostics / tree を pull し直す。 */
    kind: 'flowgraph_reloaded';
    root_dir: string;
    ok: boolean;
    error_count: number;
    warning_count: number;
    node_count: number;
   };

/** ControlEvent の kind 文字列一覧（`never` チェック用ユーティリティ）。 */
export type ControlEventKind = ControlEvent['kind'];

/**
 * 使用例:
 * ```ts
 * switch (ev.kind) {
 *   case 'channel_datum': ...; break;
 *   case 'lagged': ...; break;
 *   // ... 全 variant を書かないと下で型エラー
 *   default: assertNever(ev);
 * }
 * ```
 */
export function assertNever(x: never): never {
 throw new Error(`unreachable: unexpected ControlEvent ${JSON.stringify(x)}`);
}

// ---------------------------------------------------------------------------
// Phase δ-6: Flowgraph Control API DTOs
// ---------------------------------------------------------------------------
//
// Rust 側: src/web_interface/control/flowgraph.rs
//         + src/flowgraph/node.rs (NodeSpec / PortSpec / PropertySpec)
//         + src/flowgraph/loader/{file,diagnostic}.rs
//
// SocketType は Rust 側で文字列化されたうえで JSON に載る。例: "string" / "list<string>" / "map<json>" / "exec"。

/** ```rust
 * #[serde(rename_all = "lowercase")] pub enum Severity { Error, Warning, Info }
 * ``` */
export type FlowgraphSeverity = 'error' | 'warning' | 'info';

/** ```rust
 * #[serde(rename_all = "kebab-case")] pub enum DiagnosticCode { ... }
 * ``` */
export type FlowgraphDiagnosticCode =
 | 'toml-parse'
 | 'unknown-feature'
 | 'duplicate-node-id'
 | 'invalid-port-ref'
 | 'unresolved-node-ref'
 | 'unknown-port'
 | 'property-type-mismatch'
 | 'missing-required-property'
 | 'unknown-property'
 | 'engine-build'
 | 'io'
 | 'ambiguous-main-ref'
 | 'closed-string-literal-out-of-enum'
 | 'duplicate-enum-id'
 | 'invalid-enum-definition'
 | 'unknown-library-ref'
 | 'library-dependency-cycle';

export type FlowgraphDiagnostic = {
 severity: FlowgraphSeverity;
 code: FlowgraphDiagnosticCode;
 message: string;
 /** 発生源ファイルの (OS ネイティブ区切りの) パス。GUI ではそのまま表示する。 */
 file?: string;
 node?: string;
 hint?: string;
};

/** SocketType 文字列（`"bool" | "int" | "float" | "string" | "json" | "exec" | "list<...>" | "map<...>"`）。 */
export type FlowgraphSocketType = string;

export type FlowgraphPortDirection = 'input' | 'output';

export type FlowgraphPortSpec = {
 name: string;
 label: string;
 ty: FlowgraphSocketType;
 direction: FlowgraphPortDirection;
 is_exec: boolean;
 optional: boolean;
 /** Rust 側 `SocketValueRepr` は newtype で素の JSON として serialize される。 */
 default?: unknown;
 multi: boolean;
 description?: string;
 /** Phase λ: 閉集合 string（`ty === "string"` のとき）。 */
 closed_string_variants?: string[] | null;
 /** Phase ξ-5: node-catalog 注入。`default` から復元できた非無次元 Quantity のみ。 */
 quantity_dim?: string | null;
 quantity_unit_badge?: string | null;
 quantity_unit_full?: string | null;
};

export type FlowgraphPropertySpec = {
 name: string;
 label: string;
 ty: FlowgraphSocketType;
 default: unknown;
 required: boolean;
 description?: string;
 validator?: string;
 /** `"json"` / `"wav_path"` 等、GUI エディタのヒント。型厳密には任意文字列。 */
 ui_hint?: string;
 /**
  * Phase ο-2: enum-style constraint. When present (and `ty === "string"`),
  * the property editor renders a `<select>` dropdown instead of a plain text
  * input. Server-side node impls still validate the value themselves.
  */
 choices?: string[];
};

export type FlowgraphNodeSpec = {
 feature: string;
 title: string;
 category: string;
 description?: string;
 /** Phase λ: パレット行の一意キー（同一 feature の合成エントリ用）。 */
 palette_key?: string;
 inputs: FlowgraphPortSpec[];
 outputs: FlowgraphPortSpec[];
 properties: FlowgraphPropertySpec[];
 /**
  * Phase φ-6: Control API `POST /flowgraph/{instance}/trigger/{node_id}` で
  * このノードを外部から発火できるか。サーバ側では `NodeDescriptor::control_triggerable()`
  * の opt-in が source of truth で、node-catalog レスポンス JSON にだけこの field が注入される。
  * 古いバックエンドに繋いだ場合は undefined になる可能性があるため `?` を付ける。
  */
 control_triggerable?: boolean;
};

export type FlowgraphNodeCatalogResponse = {
 specs: FlowgraphNodeSpec[];
 count: number;
};

export type FlowgraphTreeFileEntry = {
 /** `<root>/<fq>.flowgraph.toml` の fq 部分（`/` 区切り、拡張子なし）。 */
 fq: string;
 /** root 相対パス（`/` 区切り）。 */
 path: string;
 title: string | null;
 node_count: number | null;
 edge_count: number | null;
 parse_error: string | null;
};

export type FlowgraphTreeResponse = {
 root_dir: string;
 exists: boolean;
 files: FlowgraphTreeFileEntry[];
};

/** TOML 上の 1 ノードエントリ（`[[nodes]]`）。 */
export type FlowgraphNodeEntry = {
 id: string;
 feature: string;
 /** `[x, y]` / GUI でのみ使用。 */
 position?: [number, number] | null;
 /** `toml::Table` → JSON object に変換されたもの。key = プロパティ名。 */
 properties: Record<string, unknown>;
};

export type FlowgraphEdgeEntry = {
 from: string;
 to: string;
};

export type FlowgraphFileMeta = {
 title?: string | null;
 description?: string | null;
 tags?: string[] | null;
 author?: string | null;
 name?: string | null;
 version?: string | null;
 license?: string | null;
 repos?: string | null;
 library_uses?: string[] | null;
};

export type FlowgraphEnumDef = {
 id: string;
 primitive?: string | null;
 variants: string[];
};

export type FlowgraphFileDocument = {
 meta: FlowgraphFileMeta | null;
 nodes: FlowgraphNodeEntry[];
 edges: FlowgraphEdgeEntry[];
 enums?: FlowgraphEnumDef[];
};

export type FlowgraphFileResponse = {
 fq: string;
 path: string;
 /** `.flowgraph.toml` の生テキスト。テキストエディタビュー用。 */
 raw_toml: string;
 /** TOML パース成功時のみ。 */
 parsed: FlowgraphFileDocument | null;
 parse_diagnostics: FlowgraphDiagnostic[];
};

export type FlowgraphLoadedNodeMeta = {
 feature: string;
 file: string;
 position?: [number, number] | null;
};

export type FlowgraphDiagnosticsResponse = {
 root_dir: string;
 ok: boolean;
 diagnostics: FlowgraphDiagnostic[];
 /** `fq_name` → 定義元メタ。 */
 node_meta: Record<string, FlowgraphLoadedNodeMeta>;
};

export type FlowgraphCreateFileRequest = {
 fq: string;
 initial_toml?: string;
};

export type FlowgraphPutFileRequest = {
 content: string;
};

export type FlowgraphWriteFileResponse = {
 fq: string;
 path: string;
 diagnostics: FlowgraphDiagnostic[];
 ok: boolean;
 backup: string | null;
};

export type FlowgraphOpenExternalResponse = {
 fq: string;
 path: string;
 command: string;
 spawned: boolean;
};

export type FlowgraphReloadResponse = {
 root_dir: string;
 ok: boolean;
 diagnostics: FlowgraphDiagnostic[];
 node_count: number;
};

// ---------------------------------------------------------------------------
// Phase δ-7: Fragment Share / Export / Import DTOs
// ---------------------------------------------------------------------------
//
// Rust 側:
//   - src/flowgraph/fragment/mod.rs  (CopyRequest / CopyTarget / Fragment / FragmentDangling …)
//   - src/flowgraph/fragment/paste.rs (PasteRequest / PasteTarget / PasteOptions / PasteReport …)
//   - src/flowgraph/fragment/zip_codec.rs (ZipImportOptions / ZipImportOutcome …)
//   - src/web_interface/control/flowgraph.rs (CopyResponse / PasteResponse)

export type FragmentScope = 'nodes' | 'file' | 'folder' | 'mixed';

/** `POST /flowgraph/fragment/copy` の body。 */
export type FragmentCopyRequest = {
 scope: FragmentScope;
 targets: FragmentCopyTarget[];
 origin?: string;
};

export type FragmentCopyTarget =
 | { kind: 'node'; fq_file: string; node_id: string }
 | { kind: 'file'; fq: string }
 | { kind: 'folder'; path: string };

export type FragmentHeader = {
 schema: string;
 exported_at: string;
 origin?: string | null;
 scope: FragmentScope;
};

export type FragmentFile = {
 path: string;
 meta?: FlowgraphFileMeta | null;
 nodes: FlowgraphNodeEntry[];
 edges: FlowgraphEdgeEntry[];
};

export type FragmentDangling = {
 kind: string;
 original: string;
 reason: string;
 in_file?: string | null;
 edge_from?: string | null;
 edge_to?: string | null;
 external_side?: string | null;
};

export type Fragment = {
 /** serde の `#[serde(rename = "fragment")]` に合わせる。 */
 header: FragmentHeader;
 files: FragmentFile[];
 danglings: FragmentDangling[];
};

export type FragmentCopyResponse = {
 fragment_toml: string;
 fragment: Fragment;
 file_count: number;
 dangling_count: number;
};

export type FragmentOnConflict = 'suffix' | 'skip' | 'overwrite';
export type FragmentOnConflictFile = 'overwrite' | 'skip' | 'rename' | 'merge';

export type FragmentPasteOptions = {
 on_conflict_node?: FragmentOnConflict;
 on_conflict_file?: FragmentOnConflictFile;
 position_offset?: [number, number] | null;
 /** value=null は明示的 drop、未指定は unresolved に残る。 */
 remap?: Record<string, string | null>;
};

export type FragmentPasteTarget =
 | { kind: 'file'; fq: string }
 | { kind: 'folder'; path: string };

export type FragmentPasteRequest = {
 fragment_toml: string;
 target: FragmentPasteTarget;
 options?: FragmentPasteOptions;
};

export type FragmentPasteWrittenFile = { fq: string; path: string };
export type FragmentPasteImportedNode = {
 fq_file: string;
 original_id: string;
 new_id: string;
};
export type FragmentPasteSkippedNode = {
 fq_file: string;
 original_id: string;
 reason: string;
};

export type FragmentPasteReport = {
 written_files: FragmentPasteWrittenFile[];
 imported_nodes: FragmentPasteImportedNode[];
 skipped_nodes: FragmentPasteSkippedNode[];
 skipped_files: string[];
 unresolved_danglings: FragmentDangling[];
 backups: string[];
};

export type FragmentPasteResponse = {
 report: FragmentPasteReport;
 reload_ok: boolean;
 diagnostics_count: number;
 node_count: number;
};

// ZIP Import

export type ZipImportEntry = {
 zip_path: string;
 dest_path: string;
 size: number;
};

export type ZipImportPreview = {
 kind: 'preview';
 manifest: FragmentHeader;
 file_count: number;
 entries: ZipImportEntry[];
 danglings: FragmentDangling[];
 target_prefix: string;
 conflicts: string[];
 companions: ZipImportEntry[];
};

export type ZipImportReport = {
 kind: 'report';
 manifest: FragmentHeader;
 written_files: string[];
 written_companions: string[];
 danglings_unresolved: FragmentDangling[];
 target_prefix: string;
};

export type ZipImportOutcome = ZipImportPreview | ZipImportReport;

// ---------------------------------------------------------------------------
// Phase φ-1/φ-2/φ-3: Control Table CRUD + Flowgraph Trigger DTOs
//
// Rust 側:
//   - src/web_interface/control/table.rs          (TableCatalogItem / TableFileDto 等)
//   - src/web_interface/control/flowgraph.rs      (TriggerNodeRequest / TriggerNodeResponse)
//   - src/conf/mod.rs                              (ControlTableQuickAdd)
// ---------------------------------------------------------------------------

/** `[[control_api.tables]].quick_add` のサブ設定。 */
export type ControlTableQuickAdd = {
	/** `dictionary.learn` ノードの fq ID（`"main::learn"` など）。 */
	node_id: string;
	/** 既定の `kind`（`"literal"` | `"regex"` | 省略）。 */
	kind?: string | null;
	/**
	 * Phase φ-4: `dictionary.forget` ノードの fq ID。履歴の [Undo] 用。
	 * 未設定なら GUI 側で Undo 不可として扱う。
	 */
	forget_node_id?: string | null;
};

/** `GET /api/v1/control/tables` の 1 エントリ。 */
export type TableCatalogItem = {
	/** URL / allow-list 上のキー。`/api/v1/control/table/{key}`。 */
	key: string;
	/** ディスク上の実パス（表示用）。 */
	path: string;
	label?: string | null;
	/** `"dictionary"` | `"scene-registry"` | `"generic"` | 自由文字列。 */
	role?: string | null;
	editable: boolean;
	quick_add?: ControlTableQuickAdd | null;
	/** ファイルが現在ディスクに存在するか。false でも空 Table として GET は成功する。 */
	exists: boolean;
};

export type TableCatalogResponse = {
	tables: TableCatalogItem[];
	count: number;
};

/** TSV 1 行ぶん。`values` は schema の列名 → JSON 値。 */
export type TableEntryDto = {
	row_index: number;
	values: Record<string, unknown>;
};

/** `GET /api/v1/control/table/{key}` のレスポンス。 */
export type TableFileDto = {
	key: string;
	path: string;
	/** 挿入順の列名。 */
	columns: string[];
	rows: TableEntryDto[];
	/**
	 * 現内容の blake3 ハッシュ（64 hex、prefix 無し）。
	 * mutation 系で `If-Match: b3:<content_hash>` として送ると楽観ロックが効く。
	 */
	content_hash: string;
	editable: boolean;
};

/** `PUT /api/v1/control/table/{key}` の body。 */
export type PutTableRequest = {
	/**
	 * 列順を明示する場合に指定。未指定かつ現 Table が空でなければ現 schema 列順を踏襲する。
	 * 現 Table が空（ファイル未存在など）の場合は必須。
	 */
	columns?: string[];
	rows: Array<Record<string, unknown>>;
};

/** `POST /table/{key}/entry` および `PATCH /table/{key}/entry/{row_index}` の body。 */
export type TableEntryRequest = {
	values: Record<string, unknown>;
};

/** mutation 系（PUT / POST / PATCH / DELETE）共通レスポンス。 */
export type TableMutationResponse = {
	ok: boolean;
	/** 書き込み後の新しい content_hash（次の If-Match に使う）。 */
	content_hash: string;
	row_count: number;
	affected_row_index: number | null;
};

// --- Flowgraph Trigger Endpoint (φ-2) ---------------------------------------

/**
 * `POST /api/v1/control/flowgraph/{instance_id}/trigger/{node_id}` の body。
 *
 * `inputs` の value は以下いずれかを受け付ける:
 *   - 生 JSON: `"ドクターウサギ"` / `42` / `true` / ...
 *   - 型注釈ラッパー: `{ type: "string", value: "..." }`（サーバ側で value を取り出して coerce）
 *
 * `exec_port` は複数の exec 入力を持つノード向け。省略時は最初の exec 入力を自動選択（通常 `exec_in`）。
 */
export type TriggerNodeRequest = {
	inputs?: Record<string, unknown>;
	exec_port?: string;
};

export type TriggerNodeResponse = {
	accepted: boolean;
	instance_id: string;
	node_id: string;
	feature: string;
	exec_port: string;
	/** 実際に coerce して送った override key 名（debug 用）。 */
	overridden_inputs: string[];
};

// ---------------------------------------------------------------------------
// エラー型
// ---------------------------------------------------------------------------

export class ControlApiError extends Error {
 constructor(
  public readonly status: number,
  public readonly statusText: string,
  public readonly body: unknown,
 ) {
  super(`Control API ${status} ${statusText}: ${JSON.stringify(body)}`);
  this.name = 'ControlApiError';
 }
}
