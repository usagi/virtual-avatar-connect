/**
 * Control API へのアクセスを 1 ヶ所に集めたクライアント。
 *
 * 方針:
 *   - すべて **async 関数** として露出。直接 `fetch` を呼ぶのは本ファイル内部に閉じ込める。
 *   - Bearer トークンは `auth.ts` から自動で取得・付与。
 *   - エラーは `ControlApiError` に正規化して throw する。呼び出し側は try/catch で受ける。
 *   - パスのプレフィックス `/api/v1/control` は `API_BASE` に集約（将来のバージョンアップに備える）。
 *
 * 型付けの徹底のため、**公開メソッドすべてに戻り値型を明示する**。IDE が signature をインライン表示するほど望ましい。
 */

import { buildAuthHeader } from './auth';
import {
 ControlApiError,
 type AiReloadRequest,
 type DictionaryEntryRequest,
 type DictionaryEntryResponse,
 type DictionaryContentResponse,
 type OpenExternalResponse,
 type RegexContentResponse,
 type RegexEntryRequest,
 type RegexEntryResponse,
 type OAuthAccount,
 type OAuthCancelResponse,
 type OAuthDeleteTokensResponse,
 type OAuthSessionView,
 type OAuthStartResponse,
 type IngressRequest,
 type IngressResponse,
 type PauseOutcome,
 type PauseTarget,
 type AiPersonaConfigResponse,
 type BosResponse,
 type NodePutConfigRequest,
 type NodePutConfigResponse,
 type ManagedAppMinimizeResponse,
 type ManagedAppStartResponse,
 type ManagedAppStopRequest,
 type ManagedAppStopResponse,
 type ManagedAppRestartResponse,
 type ManagedAppsResponse,
 type ProcessorConfigResponse,
 type PingResponse,
 type ProfileCloneRequest,
 type ProfileContentResponse,
 type ProfileOpResponse,
 type ProfilePutContentRequest,
 type ProfileRenameRequest,
 type ProfilesResponse,
 type ReloadRequest,
 type ReloadResponse,
 type CurrentModeResponse,
 type ModesListResponse,
 type PutCurrentModeBody,
 type RestartRequest,
 type RestartResponse,
 type ShutdownRequest,
 type ShutdownResponse,
 type RunWithDto,
 type RunWithListResponse,
 type RunWithMutationResponse,
 type StateSnapshot,
 type WhoAmIResponse,
 type FlowgraphNodeCatalogResponse,
 type FlowgraphTreeResponse,
 type FlowgraphFileResponse,
 type FlowgraphDiagnosticsResponse,
 type FlowgraphCreateFileRequest,
 type FlowgraphPutFileRequest,
 type FlowgraphWriteFileResponse,
 type FlowgraphOpenExternalResponse,
 type FlowgraphReloadResponse,
 type FragmentCopyRequest,
 type FragmentCopyResponse,
 type FragmentPasteRequest,
 type FragmentPasteResponse,
 type ZipImportOutcome,
 type TableCatalogResponse,
 type TableFileDto,
 type TableMutationResponse,
 type PutTableRequest,
 type TableEntryRequest,
 type TriggerNodeRequest,
 type TriggerNodeResponse,
} from './types';

/** `GET /flowgraph/parse-unit` の JSON。Phase ξ-5 プロパティエディタの単位検証用。 */
export type FlowgraphParseUnitResponse = {
	valid: boolean;
	dimension: string | null;
	canonical_unit: string | null;
	error: string | null;
};

const API_BASE = '/api/v1/control';

type JsonInit = Omit<RequestInit, 'body' | 'headers'> & {
 body?: unknown;
 /**
  * 追加リクエストヘッダ（`If-Match`、`X-Request-Id` など）。
  * `Accept` / `Authorization` / `Content-Type` は内部で管理するため
  * ここで指定しても上書きされる。
  */
 extraHeaders?: Record<string, string>;
};

async function request<T>(path: string, init: JsonInit = {}): Promise<T> {
 const { body, extraHeaders, ...rest } = init;
 const headers: Record<string, string> = {
  Accept: 'application/json',
  ...(extraHeaders ?? {}),
  ...buildAuthHeader(),
 };
 let finalBody: BodyInit | undefined;
 if (body !== undefined) {
  headers['Content-Type'] = 'application/json';
  finalBody = JSON.stringify(body);
 }
 const res = await fetch(`${API_BASE}${path}`, {
  ...rest,
  headers,
  body: finalBody,
 });
 if (!res.ok) {
  let errBody: unknown;
  try {
   errBody = await res.json();
  } catch {
   try {
    errBody = await res.text();
   } catch {
    errBody = null;
   }
  }
  throw new ControlApiError(res.status, res.statusText, errBody);
 }
 // 204 No Content など
 if (res.status === 204) return undefined as T;
 return (await res.json()) as T;
}

// ---------------------------------------------------------------------------
// 公開 API
// ---------------------------------------------------------------------------

export const api = {
 // --- Health ---
 ping(): Promise<PingResponse> {
  return request<PingResponse>('/ping');
 },
 whoami(): Promise<WhoAmIResponse> {
  return request<WhoAmIResponse>('/whoami');
 },

 // --- Snapshot ---
 snapshot(): Promise<StateSnapshot> {
  return request<StateSnapshot>('/snapshot');
 },

 // --- Pause / Resume ---
 pause(target: PauseTarget): Promise<PauseOutcome> {
  return request<PauseOutcome>('/pause', { method: 'POST', body: target });
 },
 resume(target: PauseTarget): Promise<PauseOutcome> {
  return request<PauseOutcome>('/resume', { method: 'POST', body: target });
 },

 // --- Reload ---
 reload(req: ReloadRequest): Promise<ReloadResponse> {
  return request<ReloadResponse>('/reload', { method: 'POST', body: req });
 },

 reloadAiPersona(id: string | null, changes: AiReloadRequest): Promise<ReloadResponse> {
  const body: ReloadRequest = { target: 'ai_persona', id, ...changes };
  return request<ReloadResponse>('/reload', { method: 'POST', body });
 },

 reloadModifyFiles(id: string | null = null): Promise<ReloadResponse> {
  return request<ReloadResponse>('/reload', {
   method: 'POST',
   body: { target: 'modify_files', id } satisfies ReloadRequest,
  });
 },

 // --- Runtime Modes ---
 modesList(): Promise<ModesListResponse> {
  return request<ModesListResponse>('/modes');
 },
 currentMode(): Promise<CurrentModeResponse> {
  return request<CurrentModeResponse>('/modes/current');
 },
 putCurrentMode(req: PutCurrentModeBody): Promise<CurrentModeResponse> {
  return request<CurrentModeResponse>('/modes/current', { method: 'PUT', body: req });
 },

 // --- OAuth (Twitch DCF) ---
 oauthStart(account: OAuthAccount): Promise<OAuthStartResponse> {
  return request<OAuthStartResponse>(`/oauth/twitch/${account}/start`, { method: 'POST' });
 },
 oauthStatus(account: OAuthAccount): Promise<OAuthSessionView> {
  return request<OAuthSessionView>(`/oauth/twitch/${account}/status`);
 },
 /**
  * `/status` は「セッションなし」時に 404 を返す。UI 初期化で「セッションがあれば表示する」用途では
  * 404 を `null` に正規化したほうが扱いやすいので、専用の変種を提供する。
  */
 async oauthStatusOrNull(account: OAuthAccount): Promise<OAuthSessionView | null> {
  try {
   return await request<OAuthSessionView>(`/oauth/twitch/${account}/status`);
  } catch (e) {
   if (e instanceof ControlApiError && e.status === 404) return null;
   throw e;
  }
 },
 oauthCancel(account: OAuthAccount): Promise<OAuthCancelResponse> {
  return request<OAuthCancelResponse>(`/oauth/twitch/${account}/cancel`, { method: 'POST' });
 },
 /**
  * 保存済みトークンファイルを破棄する。強制的に再認可を要求したい場面（別アカウントに切り替える、
  * 挙動が怪しい、など）で使う。既存 eventsub 接続のメモリ上トークンは本操作では無効化されない。
  */
 oauthDeleteTokens(account: OAuthAccount): Promise<OAuthDeleteTokensResponse> {
  return request<OAuthDeleteTokensResponse>(`/oauth/twitch/${account}/tokens`, { method: 'DELETE' });
 },
 /**
  * 任意の ChannelDatum を VAC のパイプラインへ投入する。
  * `POST /input` と違って `[[processors]]` の webinput 設定に依存せず、常に叩ける。
  */
 ingress(req: IngressRequest): Promise<IngressResponse> {
  return request<IngressResponse>('/ingress', { method: 'POST', body: req });
 },

 // --- BOS (γ-2a) ---
 /**
  * Broadcast Output Sources（OBS 向けブラウザソース一覧）を取得する。
  * `document_root` 配下の `index.html` を持つサブディレクトリがエントリとして返る。
  */
 bos(): Promise<BosResponse> {
  return request<BosResponse>('/bos');
 },

 // --- Managed Apps (γ-2b) ---
 managedApps(): Promise<ManagedAppsResponse> {
  return request<ManagedAppsResponse>('/managed_apps');
 },
 managedAppStart(id: string): Promise<ManagedAppStartResponse> {
  return request<ManagedAppStartResponse>(`/managed_apps/${encodeURIComponent(id)}/start`, { method: 'POST' });
 },
 managedAppStop(id: string, req: ManagedAppStopRequest = {}): Promise<ManagedAppStopResponse> {
  return request<ManagedAppStopResponse>(`/managed_apps/${encodeURIComponent(id)}/stop`, {
   method: 'POST',
   body: req,
  });
 },
 managedAppMinimize(id: string): Promise<ManagedAppMinimizeResponse> {
  return request<ManagedAppMinimizeResponse>(`/managed_apps/${encodeURIComponent(id)}/minimize`, {
   method: 'POST',
  });
 },
 managedAppRestart(id: string, req: ManagedAppStopRequest = {}): Promise<ManagedAppRestartResponse> {
  return request<ManagedAppRestartResponse>(`/managed_apps/${encodeURIComponent(id)}/restart`, {
   method: 'POST',
   body: req,
  });
 },

 // --- Processor / AI Persona config (γ-3a) ---
 processorConfig(index: number): Promise<ProcessorConfigResponse> {
  return request<ProcessorConfigResponse>(`/processors/${index}/config`);
 },
 aiPersonaConfig(index: number): Promise<AiPersonaConfigResponse> {
  return request<AiPersonaConfigResponse>(`/ai_personas/${index}/config`);
 },

 // --- Processor / AI Persona config update (γ-3b) ---
 putProcessorConfig(index: number, req: NodePutConfigRequest): Promise<NodePutConfigResponse> {
  return request<NodePutConfigResponse>(`/processors/${index}/config`, { method: 'PUT', body: req });
 },
 putAiPersonaConfig(index: number, req: NodePutConfigRequest): Promise<NodePutConfigResponse> {
  return request<NodePutConfigResponse>(`/ai_personas/${index}/config`, { method: 'PUT', body: req });
 },

 // --- Restart / Profiles (γ-1) ---
 profiles(): Promise<ProfilesResponse> {
  return request<ProfilesResponse>('/profiles');
 },

 // --- Profile operations (γ-4a) ---
 profileContent(filename: string): Promise<ProfileContentResponse> {
  return request<ProfileContentResponse>(`/profiles/${encodeURIComponent(filename)}/content`);
 },
 profilePutContent(filename: string, req: ProfilePutContentRequest): Promise<ProfileOpResponse> {
  return request<ProfileOpResponse>(`/profiles/${encodeURIComponent(filename)}/content`, {
   method: 'PUT',
   body: req,
  });
 },
 profileClone(req: ProfileCloneRequest): Promise<ProfileOpResponse> {
  return request<ProfileOpResponse>('/profiles/clone', { method: 'POST', body: req });
 },
 profileRename(filename: string, req: ProfileRenameRequest): Promise<ProfileOpResponse> {
  return request<ProfileOpResponse>(`/profiles/${encodeURIComponent(filename)}/rename`, {
   method: 'POST',
   body: req,
  });
 },
 profileDelete(filename: string): Promise<ProfileOpResponse> {
  return request<ProfileOpResponse>(`/profiles/${encodeURIComponent(filename)}`, { method: 'DELETE' });
 },

 // --- Modify dictionary / regex entries (γ-8a) ---
 /**
  * Modify processor の `writable_dictionary_file` に 1 行追加する。同一 (to, from) が既にあれば
  * `already_present: true` で no-op。
  */
 addDictionaryEntry(id: string, req: DictionaryEntryRequest): Promise<DictionaryEntryResponse> {
  return request<DictionaryEntryResponse>(`/modify/${encodeURIComponent(id)}/dictionary/entries`, {
   method: 'POST',
   body: req,
  });
 },
 removeDictionaryEntry(id: string, req: DictionaryEntryRequest): Promise<DictionaryEntryResponse> {
  return request<DictionaryEntryResponse>(`/modify/${encodeURIComponent(id)}/dictionary/entries`, {
   method: 'DELETE',
   body: req,
  });
 },
 addRegexEntry(id: string, req: RegexEntryRequest): Promise<RegexEntryResponse> {
  return request<RegexEntryResponse>(`/modify/${encodeURIComponent(id)}/regex/entries`, {
   method: 'POST',
   body: req,
  });
 },
 removeRegexEntry(id: string, req: RegexEntryRequest): Promise<RegexEntryResponse> {
  return request<RegexEntryResponse>(`/modify/${encodeURIComponent(id)}/regex/entries`, {
   method: 'DELETE',
   body: req,
  });
 },

 /**
  * Modify processor の writable ファイルを OS 既定アプリで開く（γ-9）。VAC プロセス側で
  * `cmd /C start` / `open` / `xdg-open` を fire-and-forget で起動する。パスはサーバ側で解決し、
  * リクエストボディは取らない（任意パスの起動を禁止）。
  */
 openModifyFileExternal(id: string, kind: 'dictionary' | 'regex'): Promise<OpenExternalResponse> {
  return request<OpenExternalResponse>(
   `/modify/${encodeURIComponent(id)}/files/${kind}/open-external`,
   { method: 'POST' },
  );
 },

 /**
  * Modify processor の全 `dictionary_files` の内容をファイル別にまとめて取得する（γ-8d）。
  * `is_writable` の立っているファイルのみ行 API（{@link addDictionaryEntry} 等）で編集可能。
  */
 getDictionaryContent(id: string): Promise<DictionaryContentResponse> {
  return request<DictionaryContentResponse>(`/modify/${encodeURIComponent(id)}/dictionary/content`);
 },
 /** 同じく全 `regex_files` をファイル別に取得する（γ-8d）。 */
 getRegexContent(id: string): Promise<RegexContentResponse> {
  return request<RegexContentResponse>(`/modify/${encodeURIComponent(id)}/regex/content`);
 },

 // --- run_with operations (γ-5a) ---
 runWithList(): Promise<RunWithListResponse> {
  return request<RunWithListResponse>('/run_with');
 },
 runWithAdd(entry: RunWithDto): Promise<RunWithMutationResponse> {
  return request<RunWithMutationResponse>('/run_with', { method: 'POST', body: entry });
 },
 runWithDelete(index: number): Promise<RunWithMutationResponse> {
  return request<RunWithMutationResponse>(`/run_with/${index}`, { method: 'DELETE' });
 },
 /**
  * VAC を再起動する。成功時は現プロセスが `graceful_ms` 後に `exit(0)` し、同じバイナリが
  * 新しい引数（= 新 conf path）で立ち上がる。GUI 側は WS の自動再接続を前提に UX を組むこと。
  */
 restart(req: RestartRequest = {}): Promise<RestartResponse> {
  return request<RestartResponse>('/restart', { method: 'POST', body: req });
 },

 /**
  * VAC を穏やかに終了させる（Phase ε-1）。成功時はサーバ側が ShutdownBroker を trigger し、
  * ManagedApp 停止 → actix graceful stop → bridges/ai/libretranslate cleanup を踏んで自然終了する。
  * GUI 側は WS の切断を検知する（restart と違い自動再接続はしない）。
  */
 shutdown(req: ShutdownRequest = {}): Promise<ShutdownResponse> {
  return request<ShutdownResponse>('/shutdown', { method: 'POST', body: req });
 },

 // --- Flowgraph (δ-6) ---
 /** 登録済み全 NodeSpec を取得する。パレット表示に使う。 */
 flowgraphNodeCatalog(): Promise<FlowgraphNodeCatalogResponse> {
  return request<FlowgraphNodeCatalogResponse>('/flowgraph/node-catalog');
 },
 /** Phase ξ-5: 単位文字列をサーバの `parse_unit` と同じルールで検証する。 */
 flowgraphParseUnit(text: string): Promise<FlowgraphParseUnitResponse> {
  const q = new URLSearchParams({ text });
  return request<FlowgraphParseUnitResponse>(`/flowgraph/parse-unit?${q.toString()}`);
 },
 /** `flowgraph_dir` 配下のファイル一覧を取得する（各ファイルの meta / node 数 / パースエラー含む）。 */
 flowgraphTree(): Promise<FlowgraphTreeResponse> {
  return request<FlowgraphTreeResponse>('/flowgraph/tree');
 },
 /** 特定 fq のファイル内容と parsed 構造を取得する。 */
 flowgraphFile(fq: string): Promise<FlowgraphFileResponse> {
  return request<FlowgraphFileResponse>(`/flowgraph/file/${encodeFq(fq)}`);
 },
 /** 現在ランタイムが保持している診断 + node_meta を取得する。 */
 flowgraphDiagnostics(): Promise<FlowgraphDiagnosticsResponse> {
  return request<FlowgraphDiagnosticsResponse>('/flowgraph/diagnostics');
 },
 /** 新規ファイル作成（省略時は空 `[meta]` テンプレート）。 */
 flowgraphCreateFile(req: FlowgraphCreateFileRequest): Promise<FlowgraphWriteFileResponse> {
  return request<FlowgraphWriteFileResponse>('/flowgraph/file', { method: 'POST', body: req });
 },
 /** ファイル全体を指定内容で置き換える。 */
 flowgraphPutFile(fq: string, req: FlowgraphPutFileRequest): Promise<FlowgraphWriteFileResponse> {
  return request<FlowgraphWriteFileResponse>(`/flowgraph/file/${encodeFq(fq)}`, {
   method: 'PUT',
   body: req,
  });
 },
 /** ファイルを `.bak` に退避してから削除する。 */
 flowgraphDeleteFile(fq: string): Promise<FlowgraphWriteFileResponse> {
  return request<FlowgraphWriteFileResponse>(`/flowgraph/file/${encodeFq(fq)}`, { method: 'DELETE' });
 },
 /** OS 既定アプリで開く（`cmd /C start` / `open` / `xdg-open`）。 */
 flowgraphOpenExternal(fq: string): Promise<FlowgraphOpenExternalResponse> {
  return request<FlowgraphOpenExternalResponse>(`/flowgraph/file/${encodeFq(fq)}/open-external`, {
   method: 'POST',
  });
 },
 /** ディスクから再ロードして診断を更新する。外部エディタ変更後の「Reload」相当。 */
 flowgraphReload(): Promise<FlowgraphReloadResponse> {
  return request<FlowgraphReloadResponse>('/flowgraph/reload', { method: 'POST' });
 },
 // --- Fragment copy / paste (δ-7) ---
 /** 指定した範囲を fragment TOML に切り出す。scope=nodes / file / folder / mixed をサポート。 */
 flowgraphFragmentCopy(req: FragmentCopyRequest): Promise<FragmentCopyResponse> {
  return request<FragmentCopyResponse>('/flowgraph/fragment/copy', {
   method: 'POST',
   body: req,
  });
 },
 /** fragment TOML をターゲットファイル / フォルダに書き戻す。衝突解消と dangling remap を含む。 */
 flowgraphFragmentPaste(req: FragmentPasteRequest): Promise<FragmentPasteResponse> {
  return request<FragmentPasteResponse>('/flowgraph/fragment/paste', {
   method: 'POST',
   body: req,
  });
 },
 // --- Fragment ZIP export / import (δ-7d) ---
 /** CopyRequest 相当で ZIP バイナリをダウンロードする。戻り値は Blob（caller が save する）。 */
 async flowgraphExportZip(req: FragmentCopyRequest): Promise<Blob> {
  const headers: Record<string, string> = {
   Accept: 'application/zip',
   'Content-Type': 'application/json',
   ...buildAuthHeader(),
  };
  const res = await fetch(`${API_BASE}/flowgraph/export/zip`, {
   method: 'POST',
   headers,
   body: JSON.stringify(req),
  });
  if (!res.ok) {
   let errBody: unknown;
   try {
    errBody = await res.json();
   } catch {
    errBody = await res.text();
   }
   throw new ControlApiError(res.status, res.statusText, errBody);
  }
  return await res.blob();
 },
 /** ZIP バイナリを import。`dry_run=true` でプレビュー取得、false で本番。 */
 async flowgraphImportZip(
  zip: Blob | ArrayBuffer | Uint8Array,
  params: {
   dry_run: boolean;
   target_prefix?: string;
   on_conflict_node?: string;
   on_conflict_file?: string;
  },
 ): Promise<ZipImportOutcome> {
  const qs = new URLSearchParams();
  qs.set('dry_run', params.dry_run ? 'true' : 'false');
  if (params.target_prefix !== undefined) qs.set('target_prefix', params.target_prefix);
  if (params.on_conflict_node) qs.set('on_conflict_node', params.on_conflict_node);
  if (params.on_conflict_file) qs.set('on_conflict_file', params.on_conflict_file);
  const headers: Record<string, string> = {
   Accept: 'application/json',
   'Content-Type': 'application/zip',
   ...buildAuthHeader(),
  };
  const body: BodyInit =
   zip instanceof Blob ? zip : zip instanceof Uint8Array ? new Blob([zip as unknown as BlobPart]) : new Blob([zip]);
  const res = await fetch(`${API_BASE}/flowgraph/import/zip?${qs.toString()}`, {
   method: 'POST',
   headers,
   body,
  });
  if (!res.ok) {
   let errBody: unknown;
   try {
    errBody = await res.json();
   } catch {
    errBody = await res.text();
   }
   throw new ControlApiError(res.status, res.statusText, errBody);
  }
  return (await res.json()) as ZipImportOutcome;
 },

 // ---------------------------------------------------------------------------
 // Phase φ-1: Control Table CRUD
 //
 // 全ての mutation 系は `If-Match: b3:<content_hash>` で楽観ロックが効く。
 // `ifMatch` 引数が省略された場合、GUI は「盲目上書き」になる点に注意
 // （サーバ側は header 未指定を許容するが、GUI コンポーネントは常に直近の
 // `content_hash` を渡すこと）。
 // ---------------------------------------------------------------------------

 listControlTables(): Promise<TableCatalogResponse> {
  return request<TableCatalogResponse>('/tables');
 },

 getControlTable(key: string): Promise<TableFileDto> {
  return request<TableFileDto>(`/table/${encodeURIComponent(key)}`);
 },

 putControlTable(key: string, body: PutTableRequest, ifMatch?: string): Promise<TableMutationResponse> {
  return request<TableMutationResponse>(`/table/${encodeURIComponent(key)}`, {
   method: 'PUT',
   body,
   extraHeaders: ifMatch ? { 'If-Match': `b3:${ifMatch}` } : undefined,
  });
 },

 postControlTableEntry(
  key: string,
  body: TableEntryRequest,
  ifMatch?: string,
 ): Promise<TableMutationResponse> {
  return request<TableMutationResponse>(`/table/${encodeURIComponent(key)}/entry`, {
   method: 'POST',
   body,
   extraHeaders: ifMatch ? { 'If-Match': `b3:${ifMatch}` } : undefined,
  });
 },

 patchControlTableEntry(
  key: string,
  rowIndex: number,
  body: TableEntryRequest,
  ifMatch?: string,
 ): Promise<TableMutationResponse> {
  return request<TableMutationResponse>(
   `/table/${encodeURIComponent(key)}/entry/${rowIndex}`,
   {
    method: 'PATCH',
    body,
    extraHeaders: ifMatch ? { 'If-Match': `b3:${ifMatch}` } : undefined,
   },
  );
 },

 deleteControlTableEntry(
  key: string,
  rowIndex: number,
  ifMatch?: string,
 ): Promise<TableMutationResponse> {
  return request<TableMutationResponse>(
   `/table/${encodeURIComponent(key)}/entry/${rowIndex}`,
   {
    method: 'DELETE',
    extraHeaders: ifMatch ? { 'If-Match': `b3:${ifMatch}` } : undefined,
   },
  );
 },

 // ---------------------------------------------------------------------------
 // Phase φ-2: Flowgraph Trigger Endpoint
 //
 // V2 は単一 instance のため `instance_id` 既定 `default`。将来のマルチプロファイル
 // 同時実行で profile 名にマップされる想定。
 // ---------------------------------------------------------------------------

 triggerFlowgraphNode(
  nodeId: string,
  body: TriggerNodeRequest = {},
  instanceId: string = 'default',
 ): Promise<TriggerNodeResponse> {
  return request<TriggerNodeResponse>(
   `/flowgraph/${encodeURIComponent(instanceId)}/trigger/${encodeURIComponent(nodeId)}`,
   { method: 'POST', body },
  );
 },
} as const;

/**
 * FQ パスを URL path にエスケープする。`chat-echo/main` のような `/` 区切りを保ちつつ、
 * 各セグメント単位で `encodeURIComponent` して安全化する。
 * `encodeURIComponent` でまとめて通すと `/` まで `%2F` になってしまい `flowgraph/file/{fq:.*}` に
 * 届かないため、セグメント毎に分割する。
 */
function encodeFq(fq: string): string {
 return fq
  .split('/')
  .filter((s) => s.length > 0)
  .map((s) => encodeURIComponent(s))
  .join('/');
}

export type VacApi = typeof api;
