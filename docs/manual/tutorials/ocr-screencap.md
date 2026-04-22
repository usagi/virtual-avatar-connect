# Tutorial: ocr-screencap

Windows のウィンドウをキャプチャして OCR にかけるサンプル。

> ソース: [`flowgraph.example/ocr-screencap/main.flowgraph.toml`](../../../flowgraph.example/ocr-screencap/main.flowgraph.toml)

## ねらい

- `screenshot.capture` でウィンドウタイトル部分一致のスクリーンショット取得
- `ocr.recognize` で Windows.Media.Ocr 経由のテキスト化
- 結果を `util.log` に流して確認

## プラットフォーム

- Windows 10/11 のみ（`screenshot.capture` / `ocr.recognize` は `#[cfg(windows)]`）。
- 日本語 OCR を使う場合は Windows の言語パックに `ja-JP` OCR を追加しておく。

## グラフ構造

```
ingress.web_input (trigger) → screenshot.capture (title="メモ帳") → ocr.recognize (lang="ja-JP") → util.log
                                      ↓ data_url
                                 util.log (base64 dataurl)
```

## 実行手順

1. Windows でメモ帳（notepad）を起動、何か日本語を入力
2. <http://127.0.0.1:57000/input> に任意の文字列を投げる（トリガーとしてのみ使用）
3. VAC ログに OCR 認識結果が出る

## 応用

- `screenshot.capture` の `title` を OBS / ゲーム名に差し替え
- `ocr.recognize` の `lang` を `en-US` など別言語に
- 後段に [`openai-persona`](./openai-persona.md) を繋いで「画面を読んで AI が喋る」配信補助ツールに

## 関連ノード

[`flowgraph.screenshot.capture`](../node-catalog.md#flowgraph-screenshot-capture) / [`flowgraph.ocr.recognize`](../node-catalog.md#flowgraph-ocr-recognize)
