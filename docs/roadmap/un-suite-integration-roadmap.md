# UN Suite Integration Roadmap

VAC v2 を `un-avatar` / `un-motion` と接続し、配信向けの実用パイプラインを完成形へ近づける計画。

`un-motion` は webcam / VMC / iFacialMocap などから motion を生成し、VMC/UDP、VRC OSC、UNMF/Z へ出力できる。`un-avatar` は `.vrm` / `.unavatar` を描画し、VMC/UDP または UNMF/Z を受けて姿勢・表情・手指を反映する。VAC はこの間に入り、Runtime Mode、OBS、Twitch、AI、Flowgraph による制御・分岐・演出を担当する。

## 方針

- 初期統合は **VMC/UDP** を正本にする。VAC には既に `motion.vmc_passthrough`、`flowgraph.ingress.vmc_udp`、`flowgraph.motion.vmc_parse`、VMC 抽出 / 送信ノードがあるため、追加依存なしで接続できる。
- `un-motion` / `un-avatar` のプロセス起動は `run_with` / Managed App / Runtime Mode desired state で扱う。
- VAC は renderer や tracking engine を内包しない。外部アプリを Flowgraph から観測・制御する hub として振る舞う。
- UNMF/Z / Zenoh は次段。VMC で実運用の線を閉じてから、第一級 `un_motion_frame` 型や Zenoh bridge の要否を判断する。

## 既定ポート前提

| アプリ | 用途 | 既定 |
|---|---|---|
| `un-motion` | VMC/UDP output | `127.0.0.1:39539` など任意 target |
| `un-avatar` | VMC/UDP input | `0.0.0.0:39539` profile 既定。ただし VAC 中継時は競合回避のため `127.0.0.1:39540` 等を推奨 |
| VRChat | OSC Avatar Parameters input | `127.0.0.1:9000` |

`un-avatar` の標準 profile は `[motion.vmc_udp].address = "0.0.0.0:39539"` を持つため、VAC を間に挟む場合は `un-avatar` 側 input port と VAC 側 bind port を分ける。

## M6: VMC/UDP hub preset

VAC が `un-motion` から VMC を受け、`un-avatar` や他 VMC consumer へ転送する。

- `conf.example-un-suite.toml`
  - `run_with` に `un-motion-supervisor` / `un-avatar-supervisor` の placeholder を置く
  - `default_runtime_mode = "daily"`
  - `streaming` mode で UN Suite apps を start
  - `daily` mode で stop / leave を選べる形にする
  - `motion.vmc_passthrough` で `0.0.0.0:39539` → `127.0.0.1:39540` を中継
- manual に接続手順を追加
- Resources / Modes GUI から Managed App と VMC route を確認できることを前提にする

## M7: Flowgraph examples for UN Suite

VMC stream を Flowgraph で観測し、配信制御へ接続する例を増やす。

- `flowgraph.example/un-suite-vmc-monitor`
  - `ingress.vmc_udp -> motion.vmc_parse -> vmc.extract_blendshape/root/bone -> util.log`
- `flowgraph.example/un-suite-streaming-mode`
  - 表情 / gesture / pose を Runtime Mode、OBS scene、Twitch chat action へ接続する例
- fixture は実 UDP を使わず、Base64 VMC payload または mocked trigger sequence で検証する

## M8: UNMF/Z / Zenoh investigation

VMC より情報量の多い UNMF/Z を VAC が扱う価値を検証する。

- `un-motion` の `zenoh_key_expr` 既定は `un-motion/frame`
- `un-avatar` の `[motion.unmotion_zenoh]` は `enabled` / `key` を持つ
- VAC で扱う場合の候補:
  - bridge: Zenoh subscribe → Flowgraph trigger
  - type: `un_motion_frame` socket または `motion_frame` の拡張
  - capability: `network`, 将来は `motion_stream`
- 先に VMC の実用性を確認し、必要な signal が不足した場合に着手する。

## M9: Control plane integration

UN Suite apps を VAC からより滑らかに扱う。

- Managed App app-specific preset
  - `un-motion-supervisor`
  - `un-avatar-supervisor`
  - `un-avatar-renderer`
- Runtime Mode から start / stop / minimize / leave を調整
- 将来: `un-motion` / `un-avatar` が Control API を持つ場合、VAC から profile selection / target addr / active avatar / wardrobe action を呼ぶ

## 完了条件

- `un-motion -> VAC -> un-avatar` の VMC/UDP route が example conf だけで説明できる
- VAC GUI で route と Runtime Mode を確認できる
- Flowgraph で motion event を OBS / Runtime Mode / AI / Twitch へ接続する例がある
- UNMF/Z 採用判断が roadmap 上で保留ではなく、実運用上の不足に基づく判断になっている
