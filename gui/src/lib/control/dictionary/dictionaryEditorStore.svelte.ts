/**
 * Phase φ-3b / GRN: Glossary Editor Pane のストア。
 *
 * Control API `/tables` と `/table/{key}` を薄くラップして、UI 側から
 * 「ロード中 / エラー / 現在のファイル内容 / content_hash」を runes で購読できるようにする。
 *
 * 方針:
 *   - **複数 Table を並行に持たない**。tab を切り替えた時点で前 Table は破棄する
 *     （content_hash の整合を単純化するため）。
 *   - mutation は Pane / Form / ConflictDialog 側から `api.*` を直接叩き、
 *     成功後に `reloadCurrent()` を呼ぶ。
 *   - 409 時の衝突ペイロード（最新サーバ状態）はストアで保持せず、ダイアログ側で
 *     一時的に持たせて「採用」ボタン押下後にストアを更新する。
 */

import { api } from '../../api';
import { ControlApiError } from '../../types';
import type {
	TableCatalogItem,
	TableCatalogResponse,
	TableFileDto,
} from '../../types';

export type DictionaryEditorPhase =
	| { kind: 'idle' }
	| { kind: 'loading-catalog' }
	| { kind: 'loading-table'; key: string }
	| { kind: 'ready' }
	| { kind: 'error'; message: string };

/**
 * `role` が "glossary" かどうかを判定。
 * 未指定 / null の場合は「汎用 Table」として Glossary Editor からは除外する。
 */
export function isDictionaryRole(item: TableCatalogItem): boolean {
	const role = (item.role ?? '').toLowerCase();
	return role === 'glossary';
}

function extractMessage(e: unknown): string {
	if (e instanceof ControlApiError) {
		const body = e.body as { detail?: string; error?: string; reason?: string } | null;
		return body?.detail ?? body?.reason ?? body?.error ?? `${e.status} ${e.statusText}`;
	}
	if (e instanceof Error) return e.message;
	return String(e);
}

class DictionaryEditorStore {
	phase: DictionaryEditorPhase = $state({ kind: 'idle' });
	catalog: TableCatalogItem[] = $state([]);
	/** Glossary Editor Pane から見えるべき Table のみ（role=="glossary"）。 */
	dictionaryTables: TableCatalogItem[] = $state([]);
	currentKey: string | null = $state(null);
	currentTable: TableFileDto | null = $state(null);

	async loadCatalog(): Promise<void> {
		this.phase = { kind: 'loading-catalog' };
		try {
			const res: TableCatalogResponse = await api.listControlTables();
			this.catalog = res.tables;
			this.dictionaryTables = res.tables.filter(isDictionaryRole);
			if (this.dictionaryTables.length === 0) {
				this.phase = { kind: 'idle' };
				this.currentKey = null;
				this.currentTable = null;
				return;
			}
			// 以前選択していた key が残っていればそのまま、そうでなければ先頭を既定に。
			const resolvedKey =
				this.currentKey && this.dictionaryTables.some((t) => t.key === this.currentKey)
					? this.currentKey
					: this.dictionaryTables[0].key;
			await this.selectTable(resolvedKey);
		} catch (e) {
			this.phase = { kind: 'error', message: extractMessage(e) };
		}
	}

	async selectTable(key: string): Promise<void> {
		this.currentKey = key;
		this.phase = { kind: 'loading-table', key };
		try {
			const dto = await api.getControlTable(key);
			this.currentTable = dto;
			this.phase = { kind: 'ready' };
		} catch (e) {
			this.phase = { kind: 'error', message: extractMessage(e) };
			this.currentTable = null;
		}
	}

	async reloadCurrent(): Promise<void> {
		if (!this.currentKey) return;
		await this.selectTable(this.currentKey);
	}

	/**
	 * 外部（Entry form / Conflict dialog）で mutation が成功した直後に、
	 * ネットワークラウンドトリップを省略して現状をすげ替える用。
	 * サーバから全文を返す API はあるが帯域の節約。
	 */
	applyTableOverride(dto: TableFileDto): void {
		if (dto.key !== this.currentKey) return;
		this.currentTable = dto;
		this.phase = { kind: 'ready' };
	}
}

export const dictionaryEditorStore = new DictionaryEditorStore();
export { extractMessage as extractControlApiMessage };
