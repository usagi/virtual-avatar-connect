/**
 * Phase VI-γ-6a: Widget foundation（素朴版）。
 *
 * 各 widget は「独立した Svelte コンポーネント + メタデータ（id, title, category）」の組として扱う。
 * 今は import して Live タブで直接使う素朴な構造だが、将来的に:
 *   - ユーザーが enable/disable を選べる
 *   - LocalStorage でレイアウトを保存
 *   - 複数インスタンス
 * などに拡張していく想定。
 *
 * ここでは registry 的な薄い型だけ用意しておき、個別 widget の **型だけ先に規定** する。
 * レンダリングは Live タブ側で行う（Svelte の都合上、components を JS リストに入れて動的 render するのは
 * 冗長になるので、まず手で import し構成する）。
 */

export type WidgetCategory = 'streaming' | 'subtitles' | 'monitoring' | 'tools' | 'other';

export interface WidgetMeta {
 id: string;
 title: string;
 category: WidgetCategory;
 /** このウィジェットが動くのにあたって要る条件の人間向け説明。 */
 requires?: string;
}

/** Live タブで並べる widget のカタログ。将来拡張で `enabled` 状態を LocalStorage と連動させる。 */
export const WIDGETS: WidgetMeta[] = [
 {
  id: 'obs-control',
  title: 'OBS Studio 連携',
  category: 'streaming',
  requires: 'OBS 30+ / obs-websocket v5',
 },
 {
  id: 'dictionary-quick-add',
  title: 'クイック辞書追加',
  category: 'tools',
  requires: 'feature="modify" かつ writable_dictionary_file 設定済みの processor',
 },
];
