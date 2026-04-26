/**
 * Phase ξ-5: Quantity ポートの次元 family（chip / handle の色分け）と tooltip 文言。
 * サーバの `Dimension::canonical()` 先頭記号（L/M/T/I/Θ/N/J/A）に基づく簡易マッピング。
 */

export type QuantityPortLike = {
	ty: string;
	label: string;
	quantity_dim?: string | null;
	quantity_unit_full?: string | null;
};

/** Svelte / CSS 用 class。`FlowgraphNodeCard` の port-row と handle に付与。 */
export function quantityFamilyClassFromDim(dim: string | null | undefined): string {
	if (!dim || dim === '1') return 'qty-family-none';
	const head = dim.split('·')[0]?.trim() ?? '';
	const sym = head.codePointAt(0);
	if (sym === undefined) return 'qty-family-mixed';
	const c = String.fromCodePoint(sym);
	switch (c) {
		case 'L':
			return 'qty-family-length';
		case 'M':
			return 'qty-family-mass';
		case 'T':
			return 'qty-family-time';
		case 'I':
			return 'qty-family-current';
		case 'Θ':
			return 'qty-family-temperature';
		case 'N':
			return 'qty-family-amount';
		case 'J':
			return 'qty-family-luminous';
		case 'A':
			return 'qty-family-angle';
		default:
			return 'qty-family-mixed';
	}
}

export function quantityPortTooltip(port: QuantityPortLike): string {
	const base = `${port.label} : ${port.ty}`;
	if (port.ty !== 'quantity') return base;
	const dim = port.quantity_dim;
	const full = port.quantity_unit_full;
	if (dim && full) {
		return `${base}\n次元: ${dim}\n単位: ${full}`;
	}
	if (dim) {
		return `${base}\n次元: ${dim}`;
	}
	return `${base}\nQuantity — 次元は上流の配線に依存（flowgraph.unit.assign 等）`;
}
