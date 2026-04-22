/**
 * Control API 用 Bearer トークンの取り扱い。
 *
 * VAC の認証ポリシーは「loopback は既定無認証、LAN は Bearer 必須」。
 * dev モード (`npm run dev`) は Vite が loopback 経由で proxy するので通常トークン不要。
 * LAN で運用するときだけ、以下のいずれかで取得したトークンを GUI に渡す必要がある:
 *
 *   1. `?token=xxx` クエリを URL に乗せる（ブラウザで手打ち or ブックマーク）
 *   2. 事前に localStorage に `vac.bearerToken` として保存
 *   3. 環境変数 `VITE_VAC_BEARER_TOKEN` を `.env.local` に設定（ビルド時固定）
 *
 * 解決順序は URL クエリ → localStorage → ビルド時定数 → 未設定。
 */

const LOCALSTORAGE_KEY = 'vac.bearerToken';
const QUERY_KEYS = ['token', 'access_token'];

/** 解決した Bearer トークン。起動時に 1 度だけ評価する。 */
let cachedToken: string | null = null;
let resolved = false;

export function getBearerToken(): string | null {
 if (resolved) return cachedToken;
 resolved = true;

 if (typeof window === 'undefined') return null;

 try {
  const params = new URLSearchParams(window.location.search);
  for (const key of QUERY_KEYS) {
   const v = params.get(key);
   if (v && v.trim()) {
    cachedToken = v.trim();
    window.localStorage.setItem(LOCALSTORAGE_KEY, cachedToken);
    return cachedToken;
   }
  }
 } catch {
  // ignore URL parse errors
 }

 try {
  const ls = window.localStorage.getItem(LOCALSTORAGE_KEY);
  if (ls && ls.trim()) {
   cachedToken = ls.trim();
   return cachedToken;
  }
 } catch {
  // ignore storage access errors (private mode 等)
 }

 const envToken = import.meta.env.VITE_VAC_BEARER_TOKEN as string | undefined;
 if (envToken && envToken.trim()) {
  cachedToken = envToken.trim();
  return cachedToken;
 }

 return null;
}

/** 明示的にトークンを差し替える（GUI の設定画面等から使う前提）。 */
export function setBearerToken(token: string | null): void {
 cachedToken = token && token.trim() ? token.trim() : null;
 resolved = true;
 if (typeof window === 'undefined') return;
 try {
  if (cachedToken) {
   window.localStorage.setItem(LOCALSTORAGE_KEY, cachedToken);
  } else {
   window.localStorage.removeItem(LOCALSTORAGE_KEY);
  }
 } catch {
  // ignore
 }
}

/** fetch に付与する `Authorization` ヘッダを組み立てる。トークンが無ければ空オブジェクト。 */
export function buildAuthHeader(): Record<string, string> {
 const t = getBearerToken();
 return t ? { Authorization: `Bearer ${t}` } : {};
}
