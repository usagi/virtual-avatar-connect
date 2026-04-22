/**
 * Phase VI-γ-1: conf 差分状態の簡易な状態機械ストア。
 *
 * γ-4a で Pipeline / Setup からの「ノード追加」「プロパティ編集」などを受けて状態を Dirty に落とし、
 * `toml_edit` による書き戻し完了で SavedRestartNeeded に遷移、再起動完了で Synced に戻る流れを作る。
 *
 * 本ファイルは γ-1 の段階では **まだ active に遷移を起こさない**。
 *   - `restart()` 成功時に `requestRestart()` を呼ぶ足場
 *   - 状態バッジやステータスバーが読み取る interface を先に固める
 *   - reduce ロジックは γ-4a でセーブ/編集イベントを繋ぎ込んだタイミングで本格稼働する
 *
 * 「機能なのに動いていない」のをドキュメントで明示しておくのが本 store の立ち位置。
 */

/** conf 編集状態のフェーズ。 */
export type ConfSyncPhase =
 /** ディスクと一致している。これが初期値。 */
 | 'synced'
 /** GUI 上で未保存の変更がある（`toml_edit` 書き戻し前）。 */
 | 'dirty'
 /** 保存完了だが、再起動しないと実効にならないフィールドに触れた。 */
 | 'saved_restart_needed'
 /** 再起動 API を叩いた直後、新プロセス待機中。 */
 | 'restart_in_flight';

export type ConfDirtyKey = {
 /** 影響するスコープの粗い分類（UI 表示用ラベルに使う）。 */
 scope: 'processors' | 'ai_personas' | 'twitch' | 'run_with' | 'browser_source' | 'other';
 /** 同一キーで上書きされるため、衝突しないドットパス風にしておく（例: `"processors[0].feature"`）。 */
 path: string;
};

/**
 * 状態機械ストア本体。`eventsStore` のようにモジュールシングルトンで運用する。
 *
 * 公開プロパティは `$state` でラップされるので、Svelte コンポーネントから `confSyncStore.phase` を
 * 読むだけで追従する。
 */
class ConfSyncStore {
 phase: ConfSyncPhase = $state('synced');
 /** 何が未保存 or 再起動待ちなのかの粒度情報。ステータスバーの tooltip 用。 */
 dirtyKeys: ConfDirtyKey[] = $state([]);
 /** 最後に `requestRestart` が呼ばれた Unix ms。ステータス表示のトリガ。 */
 lastRestartAt: number | null = $state(null);

 /** 編集発生時に γ-4a 側から呼ぶ想定のエントリ。現段階では未使用でも API だけ固めておく。 */
 markDirty(key: ConfDirtyKey): void {
  if (!this.dirtyKeys.some((k) => k.path === key.path)) {
   this.dirtyKeys = [...this.dirtyKeys, key];
  }
  if (this.phase === 'synced') {
   this.phase = 'dirty';
  }
 }

 /** 書き戻しが完了し、再起動が必要な場合に呼ぶ。現段階では未使用。 */
 markSavedRequiringRestart(): void {
  this.phase = 'saved_restart_needed';
 }

 /** 再起動 API 呼び出し直前/直後に呼んで、WS 切断 → 再接続 → Heartbeat 受信で synced に戻す流れ。 */
 requestRestart(): void {
  this.phase = 'restart_in_flight';
  this.lastRestartAt = Date.now();
 }

 /** 新プロセスとの接続が安定したら呼ぶ。 */
 markSynced(): void {
  this.phase = 'synced';
  this.dirtyKeys = [];
 }
}

export const confSyncStore = new ConfSyncStore();
