# Tutorial: tts-coeiroink

CoeiroInk で読み上げる。

> ソース: [`flowgraph.example/tts-coeiroink/main.flowgraph.toml`](../../../flowgraph.example/tts-coeiroink/main.flowgraph.toml) + [`conf.example-tts.toml`](../../../conf.example-tts.toml)

## ねらい

- `tts.speak` の `engine = "coeiroink"` を動かす
- voice 指定が `speaker_uuid:style_id` の **2 段** になることを理解する

## グラフ構造

```
ingress.web_input ─→ tts.speak (engine="coeiroink",
                                  voice="<speaker_uuid>:<style_id>",
                                  endpoint="http://127.0.0.1:50032")
                                  ├→ log(audio_path)
                                  └→ log(error)
```

## 前提

- CoeiroInk v2 / v2.x 系が起動（既定ポート 50032）
- 使いたいキャラクター・スタイルの UUID + style_id を知っている

UUID は以下のいずれかで取得:

- CoeiroInk 本体の GUI で確認
- CLI: `virtual-avatar-connect --coeiroink-speakers`（VAC が HTTP API を叩いて一覧表示）

## 実行手順

1. `flowgraph.example/tts-coeiroink/` を `flowgraph/` にコピー
2. `main.flowgraph.toml` の `voice` に `<uuid>:<style_id>` を設定、必要なら `endpoint` も
3. VAC 再起動 or Reload
4. <http://127.0.0.1:57000/input> に文字列

## 応用

- `speed` / `pitch` / `volume` を配線（CoeiroInk ドライバがスケール変換）
- 複数キャラ切替: `voice` を上流で `string.concat` / `regex.replace` 等で動的に組み立てる

## 関連ノード

[`flowgraph.tts.speak`](../node-catalog.md#flowgraph-tts-speak)
