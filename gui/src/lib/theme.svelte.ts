const STORAGE_KEY = 'vac.gui.theme';

export const VAC_THEMES = [
 {
  id: 'dr-usagi-default',
  label: 'Dr.USAGI Default',
  tone: '知的 / 工学 / 低ノイズ',
  description: '通常仕様。常駐ランタイムの管制卓として最も癖が少ない既定テーマ。',
 },
 {
  id: 'dark-crimson',
  label: 'Dark Crimson',
  tone: '赤黒金 / cool dark',
  description: '配信画面に置いて映える暗色テーマ。警告色と競合しない範囲で赤と金を使う。',
 },
 {
  id: 'light-silver',
  label: 'Light Silver',
  tone: '白青銀 / cool light',
  description: '明るい作業環境向け。清潔で硬質な印象を優先する。',
 },
 {
  id: 'soft-cute',
  label: 'Soft Cute',
  tone: 'かわいい / custom',
  description: '柔らかい色味と角丸を少し増やす。情報密度と操作性は崩さない。',
 },
] as const;

export type VacThemeId = (typeof VAC_THEMES)[number]['id'];

const DEFAULT_THEME: VacThemeId = 'dr-usagi-default';
const THEME_IDS = new Set<string>(VAC_THEMES.map((theme) => theme.id));

function normalizeThemeId(value: string | null | undefined): VacThemeId {
 return value && THEME_IDS.has(value) ? (value as VacThemeId) : DEFAULT_THEME;
}

function readStoredTheme(): VacThemeId {
 if (typeof window === 'undefined') return DEFAULT_THEME;
 return normalizeThemeId(window.localStorage.getItem(STORAGE_KEY));
}

class VacThemeStore {
 current: VacThemeId = $state(readStoredTheme());

 constructor() {
  this.apply(this.current);

  if (typeof window !== 'undefined') {
   window.addEventListener('storage', (event) => {
    if (event.key !== STORAGE_KEY) return;
    this.current = normalizeThemeId(event.newValue);
    this.apply(this.current);
   });
  }
 }

 set(themeId: VacThemeId): void {
  this.current = normalizeThemeId(themeId);
  if (typeof window !== 'undefined') {
   window.localStorage.setItem(STORAGE_KEY, this.current);
  }
  this.apply(this.current);
 }

 private apply(themeId: VacThemeId): void {
  if (typeof document === 'undefined') return;
  document.documentElement.dataset.vacTheme = themeId;
 }
}

export const vacThemeStore = new VacThemeStore();
