# Tutorial: openai-persona

AI Persona（OpenAI Responses API）を独立サービスとして運用し、その発話を Flowgraph で受けて TTS に流すサンプル。

> ソース: [`flowgraph.example/openai-persona/main.flowgraph.toml`](../../../flowgraph.example/openai-persona/main.flowgraph.toml) + [`conf.example-openai-chat.toml`](../../../conf.example-openai-chat.toml)

> **注記 (Phase χ)**: VAC の AI ペルソナは OpenAI **Responses API (`/v1/responses`)** を使う。旧 Chat Completions (`/v1/chat/completions`) 経路は撤去済み。設定ファイル名に `-chat` が残っているのは歴史的経緯で、内容は Responses API 用。

## ねらい

- `[[ai.personas]]` を常駐サービスとして起動し、`observe.triggers` に書いたチャンネルを購読させる
- AI の応答を `channel_utterance` に書き出す経路を把握する
- Flowgraph 側で「AI の発話を TTS に流す」最小サンプルを動かす

## 前提

- OpenAI API key（`VAC_OPENAI_API_KEY` 環境変数 or `conf.toml` の `api_key`）
- conf.toml に `[[ai.personas]]` を 1 つ以上定義（`conf.example-openai-chat.toml` を参照）

例（gpt-4o-mini、最小）:

```toml
[[ai.personas]]
id = "main"
model = "gpt-4o-mini"
observe.triggers = ["user"]
channel_utterance = "ai"
custom_instructions = "あなたは配信のアシスタントです..."
openai_max_output_tokens = 1024
```

例（gpt-5 系、reasoning 調整あり）:

```toml
[[ai.personas]]
id = "main"
model = "gpt-5-mini"
observe.triggers = ["user"]
channel_utterance = "ai"
custom_instructions = "あなたは配信のアシスタントです..."
openai_max_output_tokens = 2048
openai_reasoning_effort = "low"  # "low" | "medium" | "high"
# openai_store = false             # 既定 false、明示しなくても同じ
```

`gpt-5` / `gpt-5-mini` / `gpt-5-nano` といった reasoning model は、Responses API の `reasoning.effort` を介して思考量を指定できる。多くの配信用途では `"low"` で十分で、レイテンシとコストが抑えられる。

## 現時点の限界

- `flowgraph.ingress.channel`（任意のチャンネル名を購読する汎用 ingress ノード）が**未実装**
- そのため本例では **`ingress.web_input` → `tts.speak`** という縮退した形のまま（AI 発話を Flowgraph 側で受けるには δ-9 の ingress.channel 実装を待つ必要あり）

現在の実用ルートは v1 互換の channel 経路（AI が `channel_utterance = "ai"` に書く → `conf.toml` の v1 系 processor が `channel_from = "ai"` で読む）。v2 では voice と同じく「過渡期のギャップ」として扱う。

## 実行手順（現時点の縮退版）

1. `flowgraph.example/openai-persona/` を `flowgraph/` にコピー
2. `conf.toml` に `[[ai.personas]]` と OpenAI key を設定
3. VAC 起動
4. <http://127.0.0.1:57000/input> で user チャンネルに投稿 → AI が `ai` チャンネルに応答（GUI の Channels タブや WS 購読で確認可）

## 応用

- `ai.personas.decision` で reactive vs. heartbeat の発話判定を調整
- `function_calling` を有効化し、`conf.example-openai-tools.json` の tool 群を登録（`vac_emit_effect` で GUI 上にエフェクトを出す、など）

## 関連ノード

[`flowgraph.ingress.web_input`](../node-catalog.md#flowgraph-ingress-web-input) / [`flowgraph.tts.speak`](../node-catalog.md#flowgraph-tts-speak)
