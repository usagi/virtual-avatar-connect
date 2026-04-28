/**
 * Phase VI-γ-1: トップレベルのタブ構成。
 *
 * - v2 GUI: Now / Live / Modes / Flowgraph Studio / Resources / Observability / Settings
 * - URL hash (`#live` 等) で永続化。ブラウザ戻る/進むと同期。
 * - シングルトンストアで全コンポーネントに共有。
 *
 * γ-2 以降で各タブの中身を詰めていくが、γ-1 ではタブ自体の骨組みを確定させて
 * 既存パネル群は Tools タブに仮配置し、表示経路を壊さないようにする。
 *
 * δ-9 D.5: V1 `Pipeline` タブは廃止。旧 `#pipeline` URL は `flowgraph` に fallback する。
 * GUI redesign: 旧 `#setup` / `#logs` / `#tools` は新 IA の対応タブへ fallback する。
 */

export const TABS = [
 { id: 'now', label: 'Now', icon: 'N' },
 { id: 'live', label: 'Live', icon: 'L' },
 { id: 'modes', label: 'Modes', icon: 'M' },
 { id: 'flowgraph', label: 'Flowgraph Studio', icon: 'F' },
 { id: 'resources', label: 'Resources', icon: 'R' },
 { id: 'observability', label: 'Observability', icon: 'O' },
 { id: 'settings', label: 'Settings', icon: 'S' },
] as const;

export type TabId = (typeof TABS)[number]['id'];

const VALID_IDS: readonly TabId[] = TABS.map((t) => t.id);

function fromHash(): TabId {
 if (typeof window === 'undefined') return 'now';
 const h = window.location.hash.replace(/^#/, '').trim().toLowerCase();
 if (h === 'setup') return 'resources';
 if (h === 'logs') return 'observability';
 if (h === 'tools') return 'settings';
 if (h === 'pipeline') return 'flowgraph';
 return (VALID_IDS as readonly string[]).includes(h) ? (h as TabId) : 'now';
}

class TabNavStore {
 active: TabId = $state(fromHash());

 constructor() {
  if (typeof window !== 'undefined') {
   window.addEventListener('hashchange', () => {
    this.active = fromHash();
   });
  }
 }

 setActive(id: TabId): void {
  this.active = id;
  if (typeof window !== 'undefined') {
   // hash 更新は hashchange を自発的に再発火しないケースもあるのでストアは直接更新済み。
   const next = `#${id}`;
   if (window.location.hash !== next) {
    history.replaceState(null, '', next);
   }
  }
 }
}

export const tabNavStore = new TabNavStore();
