# Tutorial: voice-input

マイク → Vosk / Whisper → OS TTS の echo パイプライン。

> ソース: [`flowgraph.example/voice-input/main.flowgraph.toml`](../../../flowgraph.example/voice-input/main.flowgraph.toml) + [`conf.example-voice.toml`](../../../conf.example-voice.toml)

## ねらい

- `ingress.voice` が音声認識結果（`is_final`）をトリガーとして受け取る動作を確認
- 最小で「マイクに話しかけると OS TTS でエコー」を実現

## グラフ構造

```
ingress.voice ─→ tts.speak (engine="os", voice="")
                       ├→ log(audio_path)
                       └→ log(error)
```

## 前提

- voice **サービス**が起動していること
  - v2 でも voice は暫定的に `[[processors]] feature = "voice"` の形で `conf.toml` に書く必要がある（δ-9 で `[voice]` 化予定）
  - `conf.example-voice.toml` の「2. Voice プロセッサー設定」をコピー
- Vosk の場合: 日本語モデル（例 `vosk-model-small-ja-0.22`）をダウンロード済み、または model ID 指定で初回 DL 可
- Whisper を使う場合: `cargo build --features voice-whisper`

## 実行手順

1. `flowgraph.example/voice-input/` を `flowgraph/` にコピー
2. `conf.toml` に voice processor 設定を追加
3. OS のマイク許可を有効化（Windows のプライバシー設定）
4. VAC 起動
5. マイクに日本語で話す → 認識結果が OS TTS で再生される

## 応用

- `tts.speak` を voicevox に差し替え → 「マイク入力 → キャラクターの声で読み上げ」
- 途中に `dictionary.replace` を挟んで配信用語を正規化
- AI Persona の `observe.triggers = ["user"]` と組み合わせて「マイク → AI 応答 → TTS」（ただし `flowgraph.ingress.channel` 未実装のため AI 出力受け側は縮退形。[openai-persona](./openai-persona.md) 参照）

## `speech_floor`

長い発話中に下流の AI / Twitch / TTS 系を「スキップせず空き待ち」にしたい場合、voice 側に `speech_floor_key = "mic"` を設定し、下流処理に `respect_speech_floor = "mic"` を書く（現時点では v1 processor ベース。v2 でも同じキーが効く）。

## 関連ノード

[`flowgraph.ingress.voice`](../node-catalog.md#flowgraph-ingress-voice) / [`flowgraph.tts.speak`](../node-catalog.md#flowgraph-tts-speak)
