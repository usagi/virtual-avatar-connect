# Tutorial: tts-voicevox（VOICEVOX / AivisSpeech）

`tts.speak` で VOICEVOX Engine または AivisSpeech Engine に接続して読み上げる。

> ソース: [`flowgraph.example/tts-voicevox/main.flowgraph.toml`](../../../flowgraph.example/tts-voicevox/main.flowgraph.toml) + [`conf.example-tts.toml`](../../../conf.example-tts.toml)

## ねらい

- VOICEVOX 系 HTTP API への接続方法を学ぶ
- `voice` / `speed` / `pitch` / `endpoint` 入力の使い方を押さえる
- AivisSpeech も同じノードで動くことを確認する（`engine = "aivis_speech"` に変えるだけ）

## グラフ構造

```
ingress.web_input ─→ tts.speak (engine="voicevox",
                                  voice="1" (speaker id),
                                  speed=1.0, pitch=0.0,
                                  endpoint="http://127.0.0.1:50021")
                                  ├→ log(audio_path)
                                  └→ log(error)
```

## 前提

- VOICEVOX Engine / AivisSpeech Engine が起動していること
  - VOICEVOX 既定: <http://127.0.0.1:50021>
  - AivisSpeech 既定: <http://127.0.0.1:10101>
- speaker id を把握（CLI: `virtual-avatar-connect --voicevox-speakers` / `--aivisspeech-speakers`）

## 実行手順

1. `flowgraph.example/tts-voicevox/` を `flowgraph/` にコピー
2. `main.flowgraph.toml` の `voice` / `endpoint` / `engine` を環境に合わせて差し替え
3. VAC 再起動 or Reload
4. <http://127.0.0.1:57000/input> に文字列 → VOICEVOX / AivisSpeech が読み上げ

## 現時点の限界（`extra` 入力）

VOICEVOX / AivisSpeech には `volumeScale` / `intonationScale` / `prePhonemeLength` などの詳細パラメータがあるが、これらは `extra: Map<Json>` 入力に渡す設計。**現状 `map<json>` を構築できる literal ノードが未整備**のため、本例では `extra` は未配線。δ-9 で `flowgraph.literal.map_json` を入れる計画。

## 応用

- `speed` / `pitch` を `literal.float` で個別配線
- `save_path` を設定して wav 出力
- [`chat-filters`](./chat-filters.md) や [`openai-persona`](./openai-persona.md) の後段に差し替え

## 関連ノード

[`flowgraph.tts.speak`](../node-catalog.md#flowgraph-tts-speak)
