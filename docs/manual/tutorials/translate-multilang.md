# Tutorial: translate-multilang

入力 1 本を GAS（Google Apps Script）と LibreTranslate の **2 エンジンで並列翻訳**し、結果を別々にログする。

> ソース: [`flowgraph.example/translate-multilang/main.flowgraph.toml`](../../../flowgraph.example/translate-multilang/main.flowgraph.toml) + [`conf.example-translate.toml`](../../../conf.example-translate.toml)

## ねらい

- 1 つの ingress 発火から Flowgraph で **fan-out** する書き方
- `translate.gas` と `translate.libre` の両方を同時に試す
- それぞれの成功 / 失敗を個別に log に流す

## グラフ構造

```
ingress.web_input ─┬→ translate.gas   (ja → en)   ─┬→ log(result)
                   │                                └→ log(error)
                   └→ translate.libre (ja → ko)   ─┬→ log(result)
                                                    └→ log(error)
```

## 前提

- GAS translation を使う場合: 自分で GAS Web App をデプロイして URL を literal として書く（`conf.example-translate.toml` 参照）
- LibreTranslate を使う場合: libretranslate サーバの URL（自前 or パブリック）

## 実行手順

1. `flowgraph.example/translate-multilang/` を `flowgraph/` にコピー
2. `main.flowgraph.toml` 内の `literal.string` ノードの `value` を自分の GAS / Libre endpoint に差し替え
3. VAC 再起動 or Reload
4. <http://127.0.0.1:57000/input> に日本語を投げる → ログに英訳（GAS）と韓国語訳（Libre）が出る

## 応用

- `tts.speak` に英訳だけ流す → バイリンガル読み上げ
- `util.rate_limit` と組み合わせて過度な API 呼び出しを抑制
- `translate.libre` を `source="auto"` にしておけば入力言語を自動判定

## 関連ノード

[`flowgraph.translate.gas`](../node-catalog.md#flowgraph-translate-gas) / [`flowgraph.translate.libre`](../node-catalog.md#flowgraph-translate-libre)
