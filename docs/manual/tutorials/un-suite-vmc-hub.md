# U.N. Motion / U.N. Avatar VMC Hub

VAC は `un-motion` と `un-avatar` の間で VMC/UDP を中継し、Flowgraph / Runtime Mode / OBS / Twitch / AI へ motion event を接続できます。

初期統合では UNMF/Z / Zenoh ではなく、既存の VMC/UDP 経路を使います。

## 最小構成

```text
un-motion
  VMC/UDP output: 127.0.0.1:39539
        ↓
VAC
  [[motion.vmc_passthrough]]
  bind = "0.0.0.0:39539"
  forward_to = ["127.0.0.1:39540"]
        ↓
un-avatar
  [motion.vmc_udp]
  address = "127.0.0.1:39540"
```

`un-avatar` の標準 profile は `0.0.0.0:39539` を受けるため、VAC を間に入れる場合は port 競合を避けて `39540` などへ変更します。

## VAC 側 conf

[`conf.example-un-suite.toml`](../../../conf.example-un-suite.toml) をコピーし、`run_with.command` を実際の `un-motion-supervisor.exe` / `un-avatar-supervisor.exe` のパスへ変更します。

```toml
[[motion.vmc_passthrough]]
enabled = true
label = "un-motion-to-un-avatar"
bind = "0.0.0.0:39539"
forward_to = ["127.0.0.1:39540"]
```

Runtime Mode を使う場合、`streaming` mode で `un-motion` / `un-avatar` を start します。

```toml
[modes.streaming]
managed_apps.start = ["un-motion", "un-avatar"]
flowgraph_groups.enable = ["assistant", "streaming", "avatar", "motion", "obs"]
```

## Flowgraph で観測する

VMC packet を Flowgraph に入れる場合は、`flowgraph.ingress.vmc_udp` と `flowgraph.motion.vmc_parse` を使います。

既存 example:

- [`flowgraph.example/vmc-udp-ingress`](../../../flowgraph.example/vmc-udp-ingress)
- [`flowgraph.example/vmc-blendshape-trigger`](../../../flowgraph.example/vmc-blendshape-trigger)
- [`flowgraph.example/vmc-ai-mode-control`](../../../flowgraph.example/vmc-ai-mode-control)
- [`flowgraph.example/vmc-obs-scene-control`](../../../flowgraph.example/vmc-obs-scene-control)

## VRChat との違い

VRChat OSC Avatar Parameters は VMC/UDP ではありません。VRChat へ送る場合は、`un-motion` の VRC OSC output か、VAC の `flowgraph.vrchat.*` ノードを使います。

## 後続

UNMF/Z / Zenoh は `un-motion` と `un-avatar` の直接接続では有力ですが、VAC ではまず VMC/UDP の hub と Flowgraph 連携を安定させます。必要な信号が VMC で不足した時点で、Zenoh subscribe bridge と `un_motion_frame` 型を検討します。
