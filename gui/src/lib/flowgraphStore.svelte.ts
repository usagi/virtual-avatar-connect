/**
 * Phase δ-6e: Flowgraph タブの状態管理ストア。
 *
 * 責務:
 *   - サーバから取得する各種リソース（tree / node catalog / current file / diagnostics）を保持。
 *   - 「選択中ファイル」「選択中ノード」の UI 状態を保持。
 *   - `ControlEvent::FlowgraphReloaded` を購読し、tree / diagnostics / 開いているファイルを自動で再取得。
 *   - 編集 API（POST/PUT/DELETE / open-external）への薄い委譲。呼び出し側（UI）は
 *     `store.saveCurrent(...)` のような明示的なメソッドだけを叩けば済む設計。
 *
 * 非責務:
 *   - TOML のシリアライズ。GUI 側では parsed 構造 ↔ raw text の変換を行なわず、**raw TOML を
 *     single source of truth** として扱う。ノード位置やプロパティの編集は parsed を JSON で更新し、
 *     保存時に再シリアライズする（toml_edit を JS で扱うのは重いので minimal に）。
 *   - 実行トリガ。δ-6e は GUI 側から flowgraph を動かすことはしない（δ-8 で run_forever を接続する予定）。
 */

import { api } from './api';
import { eventsStore } from './events.svelte';
import { toastStore } from './toasts.svelte';
import {
 ControlApiError,
 type FlowgraphDiagnostic,
 type FlowgraphDiagnosticsResponse,
 type FlowgraphFileResponse,
 type FlowgraphNodeCatalogResponse,
 type FlowgraphNodeSpec,
 type FlowgraphTreeResponse,
 type FragmentCopyRequest,
 type FragmentCopyResponse,
 type FragmentPasteRequest,
 type FragmentPasteResponse,
 type ZipImportOutcome,
} from './types';

/** 書き戻し中フラグなど非同期状態をまとめて扱う列挙。 */
export type FlowgraphFetchState = 'idle' | 'loading' | 'ok' | 'error';

class FlowgraphStore {
 /** `GET /flowgraph/tree` の結果。初期 null はまだ取得していない（初回ロード前）。 */
 tree: FlowgraphTreeResponse | null = $state(null);
 treeState: FlowgraphFetchState = $state('idle');
 treeError: string | null = $state(null);

 /** `GET /flowgraph/node-catalog` の結果。カタログは更新頻度が低いので一度 fetch したら使い回す。 */
 catalog: FlowgraphNodeCatalogResponse | null = $state(null);
 catalogState: FlowgraphFetchState = $state('idle');

 /** `GET /flowgraph/diagnostics` の結果。`FlowgraphReloaded` WS でも再取得する。 */
 diagnostics: FlowgraphDiagnosticsResponse | null = $state(null);

 /** 現在選択中 / 編集中のファイル。null のときは何も開いていない。 */
 currentFq: string | null = $state(null);
 currentFile: FlowgraphFileResponse | null = $state(null);
 currentState: FlowgraphFetchState = $state('idle');
 currentError: string | null = $state(null);

 /**
  * 未保存の変更があるか。`currentFile.raw_toml` と `draftToml` が乖離したら true。
  * GUI の「Save」ボタン活性化や「閉じる前の確認」に使う。
  */
 draftToml: string | null = $state(null);
 /** ノード位置やプロパティの GUI 編集を parsed 層で持つ。保存時に raw へ再シリアライズ。 */
 draftNodes: FlowgraphDraftNode[] | null = $state(null);
 draftEdges: FlowgraphDraftEdge[] | null = $state(null);

 /** いま選択しているキャンバス上のノード ID。プロパティエディタが参照する。 */
 selectedNodeId: string | null = $state(null);

 /** 保存中 / 削除中 / 新規作成中のスピナー用。同時に複数走らない前提。 */
 mutating: boolean = $state(false);

 #wsUnsubscribe: (() => void) | null = null;

 // -------------------------------------------------------------------------
 // Reads
 // -------------------------------------------------------------------------

 async refreshAll(): Promise<void> {
  await Promise.all([this.refreshTree(), this.refreshCatalog(), this.refreshDiagnostics()]);
 }

 async refreshTree(): Promise<void> {
  this.treeState = 'loading';
  try {
   this.tree = await api.flowgraphTree();
   this.treeState = 'ok';
   this.treeError = null;
  } catch (e) {
   this.treeState = 'error';
   this.treeError = describeError(e);
  }
 }

 async refreshCatalog(force = false): Promise<void> {
  if (this.catalog && !force) return;
  this.catalogState = 'loading';
  try {
   this.catalog = await api.flowgraphNodeCatalog();
   this.catalogState = 'ok';
  } catch (e) {
   this.catalogState = 'error';
   toastStore.warn('ノードカタログ取得失敗', describeError(e));
  }
 }

 async refreshDiagnostics(): Promise<void> {
  try {
   this.diagnostics = await api.flowgraphDiagnostics();
  } catch (e) {
   // diagnostics は `flowgraph_dir` 未設定時 500 を返すので、失敗は warn トーストだけに留めて握り潰す。
   console.warn('[flowgraph] diagnostics 取得失敗', e);
  }
 }

 // -------------------------------------------------------------------------
 // Current file
 // -------------------------------------------------------------------------

 async openFile(fq: string): Promise<void> {
  this.currentFq = fq;
  this.currentState = 'loading';
  this.currentError = null;
  this.selectedNodeId = null;
  try {
   const resp = await api.flowgraphFile(fq);
   this.currentFile = resp;
   this.draftToml = resp.raw_toml;
   this.draftNodes = resp.parsed
    ? resp.parsed.nodes.map((n) => ({
       id: n.id,
       feature: n.feature,
       position: n.position ?? null,
       properties: { ...n.properties },
      }))
    : null;
   this.draftEdges = resp.parsed ? resp.parsed.edges.map((e) => ({ from: e.from, to: e.to })) : null;
   this.currentState = 'ok';
  } catch (e) {
   this.currentState = 'error';
   this.currentError = describeError(e);
   toastStore.error('ファイル読み込み失敗', describeError(e));
  }
 }

 closeFile(): void {
  this.currentFq = null;
  this.currentFile = null;
  this.draftToml = null;
  this.draftNodes = null;
  this.draftEdges = null;
  this.selectedNodeId = null;
  this.currentState = 'idle';
  this.currentError = null;
 }

 /** draft 層のノード位置を更新する。Svelte Flow から呼ぶ。 */
 updateNodePosition(id: string, x: number, y: number): void {
  if (!this.draftNodes) return;
  const next = this.draftNodes.map((n) => (n.id === id ? { ...n, position: [x, y] as [number, number] } : n));
  this.draftNodes = next;
 }

 updateNodeProperty(id: string, key: string, value: unknown): void {
  if (!this.draftNodes) return;
  const next = this.draftNodes.map((n) => (n.id === id ? { ...n, properties: { ...n.properties, [key]: value } } : n));
  this.draftNodes = next;
 }

 removeNodeProperty(id: string, key: string): void {
  if (!this.draftNodes) return;
  const next = this.draftNodes.map((n) => {
   if (n.id !== id) return n;
   const props = { ...n.properties };
   delete props[key];
   return { ...n, properties: props };
  });
  this.draftNodes = next;
 }

 addNode(node: FlowgraphDraftNode): void {
  if (!this.draftNodes) this.draftNodes = [];
  this.draftNodes = [...this.draftNodes, node];
 }

 removeNode(id: string): void {
  if (!this.draftNodes) return;
  this.draftNodes = this.draftNodes.filter((n) => n.id !== id);
  if (this.draftEdges) {
   this.draftEdges = this.draftEdges.filter((e) => !edgeMentionsNode(e, id));
  }
  if (this.selectedNodeId === id) this.selectedNodeId = null;
 }

 addEdge(from: string, to: string): void {
  if (!this.draftEdges) this.draftEdges = [];
  if (this.draftEdges.some((e) => e.from === from && e.to === to)) return;
  this.draftEdges = [...this.draftEdges, { from, to }];
 }

 removeEdge(from: string, to: string): void {
  if (!this.draftEdges) return;
  this.draftEdges = this.draftEdges.filter((e) => !(e.from === from && e.to === to));
 }

 /** 現在の draft を TOML に書き戻してディスクへ保存する。 */
 async saveCurrent(): Promise<boolean> {
  if (!this.currentFq || !this.draftNodes || !this.draftEdges) return false;
  const nextToml = serializeFlowgraph({
   meta: this.currentFile?.parsed?.meta ?? null,
   nodes: this.draftNodes,
   edges: this.draftEdges,
  });
  this.mutating = true;
  try {
   const resp = await api.flowgraphPutFile(this.currentFq, { content: nextToml });
   this.mutating = false;
   const severity = resp.ok ? 'success' : 'warn';
   if (severity === 'success') {
    toastStore.success('Flowgraph を保存しました', `fq=${resp.fq}`);
   } else {
    toastStore.warn(
     'Flowgraph を保存しましたが検証エラーがあります',
     `${resp.diagnostics.length} 件の診断`,
    );
   }
   // 保存後に最新ステートを引き直す（raw_toml がサーバ側の正規化を含む可能性があるため）。
   await this.openFile(this.currentFq);
   await this.refreshTree();
   await this.refreshDiagnostics();
   return true;
  } catch (e) {
   this.mutating = false;
   toastStore.error('保存失敗', describeError(e));
   return false;
  }
 }

 async saveRawToml(raw: string): Promise<boolean> {
  if (!this.currentFq) return false;
  this.mutating = true;
  try {
   const resp = await api.flowgraphPutFile(this.currentFq, { content: raw });
   this.mutating = false;
   if (resp.ok) toastStore.success('Flowgraph を保存しました', `fq=${resp.fq}`);
   else toastStore.warn('保存しましたが検証エラーあり', `${resp.diagnostics.length} 件`);
   await this.openFile(this.currentFq);
   await this.refreshTree();
   await this.refreshDiagnostics();
   return true;
  } catch (e) {
   this.mutating = false;
   toastStore.error('保存失敗', describeError(e));
   return false;
  }
 }

 // -------------------------------------------------------------------------
 // File CRUD
 // -------------------------------------------------------------------------

 async createFile(fq: string, initialToml?: string): Promise<boolean> {
  this.mutating = true;
  try {
   await api.flowgraphCreateFile({ fq, initial_toml: initialToml });
   this.mutating = false;
   toastStore.success('ファイルを作成しました', `fq=${fq}`);
   await Promise.all([this.refreshTree(), this.refreshDiagnostics()]);
   await this.openFile(fq);
   return true;
  } catch (e) {
   this.mutating = false;
   toastStore.error('ファイル作成失敗', describeError(e));
   return false;
  }
 }

 async deleteFile(fq: string): Promise<boolean> {
  this.mutating = true;
  try {
   const resp = await api.flowgraphDeleteFile(fq);
   this.mutating = false;
   toastStore.success('ファイルを削除しました', `backup=${resp.backup ?? '(none)'}`);
   if (this.currentFq === fq) this.closeFile();
   await Promise.all([this.refreshTree(), this.refreshDiagnostics()]);
   return true;
  } catch (e) {
   this.mutating = false;
   toastStore.error('削除失敗', describeError(e));
   return false;
  }
 }

 async openExternal(fq: string): Promise<void> {
  try {
   const resp = await api.flowgraphOpenExternal(fq);
   if (resp.spawned) toastStore.info('外部エディタで開きました', resp.command);
   else toastStore.warn('外部エディタ起動失敗', resp.command);
  } catch (e) {
   toastStore.error('外部起動失敗', describeError(e));
  }
 }

 async reloadFromDisk(): Promise<void> {
  try {
   const resp = await api.flowgraphReload();
   if (resp.ok) toastStore.success('Reloaded', `${resp.node_count} ノード`);
   else toastStore.warn('Reloaded (診断あり)', `${resp.diagnostics.length} 件`);
   await Promise.all([this.refreshTree(), this.refreshDiagnostics()]);
   if (this.currentFq) await this.openFile(this.currentFq);
  } catch (e) {
   toastStore.error('Reload 失敗', describeError(e));
  }
 }

 // -------------------------------------------------------------------------
 // Fragment share / export / import (δ-7)
 // -------------------------------------------------------------------------

 /** copy API を叩いて fragment を取得する。成功時はクリップボード書き込みも試みる。 */
 async fragmentCopy(req: FragmentCopyRequest): Promise<FragmentCopyResponse | null> {
  try {
   const resp = await api.flowgraphFragmentCopy(req);
   try {
    await navigator.clipboard?.writeText(resp.fragment_toml);
    toastStore.success(
     'Fragment をクリップボードにコピーしました',
     `${resp.file_count} ファイル / dangling ${resp.dangling_count}`,
    );
   } catch {
    toastStore.info(
     'Fragment を生成しました（クリップボード書き込み不可）',
     `${resp.file_count} ファイル`,
    );
   }
   return resp;
  } catch (e) {
   toastStore.error('Copy 失敗', describeError(e));
   return null;
  }
 }

 /** 現在開いているファイルの全体を fragment として copy する（簡易ヘルパ）。 */
 async fragmentCopyCurrentFile(): Promise<FragmentCopyResponse | null> {
  if (!this.currentFq) {
   toastStore.warn('ファイル未選択', 'copy するファイルを開いてください');
   return null;
  }
  return this.fragmentCopy({
   scope: 'file',
   targets: [{ kind: 'file', fq: this.currentFq }],
   origin: this.currentFq,
  });
 }

 /** 選択ノード 1 個を fragment 化（複数選択対応は palette 側の将来拡張）。 */
 async fragmentCopySelectedNode(): Promise<FragmentCopyResponse | null> {
  if (!this.currentFq || !this.selectedNodeId) {
   toastStore.warn('ノード未選択', 'copy するノードを選択してください');
   return null;
  }
  return this.fragmentCopy({
   scope: 'nodes',
   targets: [{ kind: 'node', fq_file: this.currentFq, node_id: this.selectedNodeId }],
   origin: this.currentFq,
  });
 }

 async fragmentPaste(req: FragmentPasteRequest): Promise<FragmentPasteResponse | null> {
  this.mutating = true;
  try {
   const resp = await api.flowgraphFragmentPaste(req);
   this.mutating = false;
   const r = resp.report;
   const unresolved = r.unresolved_danglings.length;
   const skipped = r.skipped_nodes.length + r.skipped_files.length;
   if (resp.reload_ok && unresolved === 0 && skipped === 0) {
    toastStore.success(
     'Paste しました',
     `${r.imported_nodes.length} ノード / ${r.written_files.length} ファイル`,
    );
   } else {
    toastStore.warn(
     'Paste 完了（要注意）',
     `imported=${r.imported_nodes.length} skipped=${skipped} unresolved=${unresolved} diag=${resp.diagnostics_count}`,
    );
   }
   await Promise.all([this.refreshTree(), this.refreshDiagnostics()]);
   if (this.currentFq) await this.openFile(this.currentFq);
   return resp;
  } catch (e) {
   this.mutating = false;
   toastStore.error('Paste 失敗', describeError(e));
   return null;
  }
 }

 /** ZIP export を実行して Blob を返す（caller が download 処理する）。 */
 async exportZip(req: FragmentCopyRequest): Promise<Blob | null> {
  try {
   const blob = await api.flowgraphExportZip(req);
   toastStore.success('ZIP を生成しました', `${(blob.size / 1024).toFixed(1)} KiB`);
   return blob;
  } catch (e) {
   toastStore.error('Export ZIP 失敗', describeError(e));
   return null;
  }
 }

 async importZip(
  zip: Blob,
  params: {
   dry_run: boolean;
   target_prefix?: string;
   on_conflict_node?: string;
   on_conflict_file?: string;
  },
 ): Promise<ZipImportOutcome | null> {
  this.mutating = true;
  try {
   const outcome = await api.flowgraphImportZip(zip, params);
   this.mutating = false;
   if (outcome.kind === 'preview') {
    toastStore.info(
     'Import preview',
     `${outcome.file_count} ファイル / conflicts=${outcome.conflicts.length}`,
    );
   } else {
    toastStore.success(
     'Import 完了',
     `${outcome.written_files.length} ファイル / companions=${outcome.written_companions.length}`,
    );
    await Promise.all([this.refreshTree(), this.refreshDiagnostics()]);
   }
   return outcome;
  } catch (e) {
   this.mutating = false;
   toastStore.error('Import ZIP 失敗', describeError(e));
   return null;
  }
 }

 // -------------------------------------------------------------------------
 // Feature lookup helpers
 // -------------------------------------------------------------------------

 findSpec(feature: string): FlowgraphNodeSpec | undefined {
  return this.catalog?.specs.find((s) => s.feature === feature);
 }

 groupedCatalog(): Array<{ category: string; specs: FlowgraphNodeSpec[] }> {
  // 一時バッファは普通のオブジェクトで済む（reactive state ではないので SvelteMap 不要）。
  const groups: Record<string, FlowgraphNodeSpec[]> = Object.create(null);
  for (const spec of this.catalog?.specs ?? []) {
   const key = spec.category || 'misc';
   if (!groups[key]) groups[key] = [];
   groups[key].push(spec);
  }
  return Object.entries(groups)
   .map(([category, specs]) => ({
    category,
    specs: specs.slice().sort((a, b) => a.title.localeCompare(b.title)),
   }))
   .sort((a, b) => a.category.localeCompare(b.category));
 }

 // -------------------------------------------------------------------------
 // WS subscription
 // -------------------------------------------------------------------------

 /** 一度だけ呼んで WS 購読を張る。`FlowgraphTab` がマウント時に呼ぶ。 */
 attachWsSubscriber(): void {
  if (this.#wsUnsubscribe) return;
  this.#wsUnsubscribe = eventsStore.subscribe((ts) => {
   if (ts.event.kind !== 'flowgraph_reloaded') return;
   const ev = ts.event;
   if (ev.ok) {
    toastStore.info('Flowgraph reloaded', `${ev.node_count} ノード`);
   } else {
    toastStore.warn('Flowgraph reload に診断', `err=${ev.error_count} warn=${ev.warning_count}`);
   }
   // diagnostics と tree は必ず引き直す。開いているファイルがあればそれも再取得する。
   void Promise.all([this.refreshTree(), this.refreshDiagnostics()]);
   if (this.currentFq) void this.openFile(this.currentFq);
  });
 }

 detachWsSubscriber(): void {
  if (this.#wsUnsubscribe) {
   this.#wsUnsubscribe();
   this.#wsUnsubscribe = null;
  }
 }
}

// ---------------------------------------------------------------------------
// Draft types & helpers
// ---------------------------------------------------------------------------

export type FlowgraphDraftNode = {
 id: string;
 feature: string;
 position: [number, number] | null;
 properties: Record<string, unknown>;
};

export type FlowgraphDraftEdge = {
 from: string;
 to: string;
};

function edgeMentionsNode(edge: FlowgraphDraftEdge, nodeId: string): boolean {
 return parsePortRef(edge.from).nodeId === nodeId || parsePortRef(edge.to).nodeId === nodeId;
}

/**
 * `"node:port"` / `"path::node:port"` を decomposition。spec §7 の parse_port_ref を GUI 側で簡易実装。
 * 完全な再現は不要で、ノード ID の抽出だけに使う。
 */
export function parsePortRef(ref: string): { path: string; nodeId: string; port: string } {
 const lastColon = findLastSinglePortColon(ref);
 if (lastColon < 0) return { path: '', nodeId: ref, port: '' };
 const head = ref.slice(0, lastColon);
 const port = ref.slice(lastColon + 1);
 const pathSepIdx = head.lastIndexOf('::');
 if (pathSepIdx < 0) return { path: '', nodeId: head, port };
 const path = head.slice(0, pathSepIdx);
 const nodeId = head.slice(pathSepIdx + 2);
 return { path, nodeId, port };
}

/** `::` 以外の単独 `:` の位置を末尾から探す。 */
function findLastSinglePortColon(s: string): number {
 for (let i = s.length - 1; i >= 0; i--) {
  if (s[i] !== ':') continue;
  if (i > 0 && s[i - 1] === ':') {
   i -= 1;
   continue;
  }
  if (i + 1 < s.length && s[i + 1] === ':') continue;
  return i;
 }
 return -1;
}

/**
 * draft の nodes / edges を `.flowgraph.toml` 形式に素朴にシリアライズする。
 *
 * 完全な format-preserving edit は行わず、nodes の位置・feature・id・properties と edges の from/to を
 * 機械的に TOML で書き出す。元ファイルの meta はそのまま保持する（title / description / tags）。
 *
 * この方針により、`toml_edit` を JS 側に持ち込まずに済む。コメントなどは失われるが、
 * δ-6e では GUI 編集中心なので許容する（外部エディタで直接書いた場合は PUT を使わず open-external で開く UX）。
 */
export function serializeFlowgraph(doc: {
 meta: { title?: string | null; description?: string | null; tags?: string[] | null } | null;
 nodes: FlowgraphDraftNode[];
 edges: FlowgraphDraftEdge[];
}): string {
 const lines: string[] = [];
 if (doc.meta && (doc.meta.title || doc.meta.description || (doc.meta.tags && doc.meta.tags.length > 0))) {
  lines.push('[meta]');
  if (doc.meta.title) lines.push(`title = ${tomlString(doc.meta.title)}`);
  if (doc.meta.description) lines.push(`description = ${tomlString(doc.meta.description)}`);
  if (doc.meta.tags && doc.meta.tags.length > 0) {
   lines.push(`tags = [${doc.meta.tags.map(tomlString).join(', ')}]`);
  }
  lines.push('');
 }

 for (const node of doc.nodes) {
  lines.push('[[nodes]]');
  lines.push(`id = ${tomlString(node.id)}`);
  lines.push(`feature = ${tomlString(node.feature)}`);
  if (node.position) {
   lines.push(`position = [${formatNum(node.position[0])}, ${formatNum(node.position[1])}]`);
  }
  const propKeys = Object.keys(node.properties);
  if (propKeys.length > 0) {
   lines.push('[nodes.properties]');
   for (const k of propKeys) {
    lines.push(`${tomlKey(k)} = ${tomlValue(node.properties[k])}`);
   }
  }
  lines.push('');
 }

 for (const edge of doc.edges) {
  lines.push('[[edges]]');
  lines.push(`from = ${tomlString(edge.from)}`);
  lines.push(`to = ${tomlString(edge.to)}`);
  lines.push('');
 }

 return lines.join('\n').replace(/\n{3,}$/, '\n\n').trimEnd() + '\n';
}

function tomlString(s: string): string {
 // ダブルクォート文字列化。バックスラッシュ・ダブルクォート・改行のみエスケープ。
 return `"${s
  .replace(/\\/g, '\\\\')
  .replace(/"/g, '\\"')
  .replace(/\n/g, '\\n')
  .replace(/\r/g, '\\r')
  .replace(/\t/g, '\\t')}"`;
}

function tomlKey(k: string): string {
 // bare key に合致しない場合は quoted key にする。
 if (/^[A-Za-z0-9_-]+$/.test(k)) return k;
 return tomlString(k);
}

function tomlValue(v: unknown): string {
 if (v === null || v === undefined) return '""';
 if (typeof v === 'string') return tomlString(v);
 if (typeof v === 'boolean') return v ? 'true' : 'false';
 if (typeof v === 'number') return Number.isInteger(v) ? String(v) : formatNum(v);
 if (Array.isArray(v)) {
  return `[${v.map(tomlValue).join(', ')}]`;
 }
 if (typeof v === 'object') {
  // inline table。ネスト深いケースは想定しない（GUI 側プロパティ編集の浅い object 用）。
  const entries = Object.entries(v as Record<string, unknown>);
  return `{ ${entries.map(([k, vv]) => `${tomlKey(k)} = ${tomlValue(vv)}`).join(', ')} }`;
 }
 return tomlString(String(v));
}

function formatNum(n: number): string {
 if (!Number.isFinite(n)) return '0';
 // 極小値や極大値でも TOML 的に読める表現。
 if (Number.isInteger(n)) return `${n}.0`;
 return String(n);
}

// ---------------------------------------------------------------------------
// Diagnostics helpers
// ---------------------------------------------------------------------------

export function summarizeDiagnostics(diags: FlowgraphDiagnostic[] | undefined): {
 error: number;
 warning: number;
 info: number;
} {
 const out = { error: 0, warning: 0, info: 0 };
 for (const d of diags ?? []) out[d.severity] += 1;
 return out;
}

function describeError(e: unknown): string {
 if (e instanceof ControlApiError) {
  const detail =
   typeof e.body === 'object' && e.body && 'error' in (e.body as Record<string, unknown>)
    ? String((e.body as { error?: unknown }).error)
    : e.statusText;
  return `${e.status} ${detail}`;
 }
 if (e instanceof Error) return e.message;
 return String(e);
}

export const flowgraphStore = new FlowgraphStore();
