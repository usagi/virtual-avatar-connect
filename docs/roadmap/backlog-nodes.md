# Flowgraph Node Backlog

「あると便利」だが現行フェーズ（η / φ）のスコープ外のノード設計メモを集約する。
実装時はそれぞれ独立フェーズ（χ / ψ / …）として切り出すか、別 feature PR として処理する。

関連: [src/flowgraph/registry.rs](../../src/flowgraph/registry.rs) / [src/flowgraph/nodes/](../../src/flowgraph/nodes/)
/ [phase-phi-control-api-dictionary-editor.md](phase-phi-control-api-dictionary-editor.md)
/ [docs/manual/node-catalog.md](../manual/node-catalog.md)

---

## 1. `flowgraph.util.timer_interval` — 周期タイマー（新規、未実装）

> **Status**: **Phase ο-4 として昇格予定**。以下の仕様ドラフトはそのまま再利用される。
> 詳細とサブフェーズ位置づけ: [`phase-omicron-flowgraph-enhancement.md`](phase-omicron-flowgraph-enhancement.md) §3.4 / §6 ο-4。

### 1.1 背景

現行 `flowgraph.util.delay` は **1-shot**（`exec_in` 発火から `delay_ms` 後に `exec_out` を 1 回発火）で、
**定期発火 source** が存在しない。以下のユースケースで現状代替手段がなく、外部 cron か
host 側 scheduler に漏らす必要がある。

- N 秒おきに Twitch Helix `validate_token` を叩いてトークン失効を検知
- 30 秒おきに OBS Scene 切替をポーリング
- デバッグ時に「一定間隔で既存ノードを発火させて挙動を観察する」ようなスモーク用途
- M 分おきに state.bool を反転してアバター口パク擬似信号を作る

`flowgraph.util.rate_limit` はトークンバケットによる **gate** であり source ではない。

### 1.2 ノード仕様（ドラフト）

- `feature`: `flowgraph.util.timer_interval`（検討中、`flowgraph.util.interval` / `flowgraph.util.tick` も候補）
- 種別: **StatefulNode**（self-trigger 機構で `ctx.trigger` を使う）

#### 入力

| port | type | default | description |
|------|------|---------|-------------|
| `enabled` | Bool | `true` | `false` の間は tick をスケジュールしない。`true ← false` 遷移でタイマー再始動 |
| `interval_sec` | Float | `1.0` | tick 間隔（秒）。`0.01` 未満は 10ms に clamp（runtime 保護） |
| `__tick__` | Exec (internal) | — | engine 専用の再開入口（`DelayNode` の `__resume__` パターン踏襲） |
| `__pending_id__` | Int (internal) | `-1` | TriggerEvent override で渡す tick id |

#### 出力

| port | type | description |
|------|------|-------------|
| `on_tick` | Exec | tick 到来で発火 |
| `count` | Int | 累積 tick 数（`enabled=true` の間にどれだけ発火したか） |
| `elapsed_sec` | Float | 前回 tick（またはノード起動）からの経過秒数 |

#### State

```rust
struct TimerIntervalState {
    next_id: u64,
    tick_count: i64,
    armed_id: Option<u64>,       // 現在 arm 中の tick id（dup fire 防止）
    last_fired_at: Option<Instant>,
    last_enabled: bool,          // enabled edge 検出用
}
```

### 1.3 挙動

1. **初回 compute（generation 0 の data pull）**:
   - `enabled=true` なら `ctx.trigger` で `__tick__` を `interval_sec` 後にスケジュール。
   - State に `armed_id=Some(id)` を記録。
2. **`__tick__` 発火**:
   - `pending_id` が `armed_id` と一致 & `enabled=true` なら `on_tick` を emit、`tick_count += 1`、次 tick を schedule。
   - 不一致（stale）/ `enabled=false` なら黙ってドロップ、再 arm もしない。
3. **`enabled` transition `false → true`**（次の compute で検出）:
   - `armed_id=None` の状態で呼ばれたら、新規 tick を schedule。
4. **`interval_sec` 動的変更**:
   - 次回 schedule から新 interval を使用。現在 pending 中の tick は干渉しない（早期 cancel は実装複雑なので割り切り）。

### 1.4 実装ヒント（[src/flowgraph/nodes/delay.rs](../../src/flowgraph/nodes/delay.rs) パターン踏襲）

```rust
// pseudo
async fn compute(&self, state, _props, inputs, fired, ctx) -> NodeOutput {
    let enabled = get_optional_bool(inputs, "enabled", true)?;
    let interval_sec = get_optional_float(inputs, "interval_sec", 1.0)?
        .max(0.01); // runtime 保護

    let trigger = ctx.trigger.clone(); // 1-shot execute の場合 None
    let st: &mut TimerIntervalState = state.downcast_mut().unwrap();

    // __tick__ 発火判定
    let tick_fired = fired.contains("__tick__");

    if tick_fired {
        let incoming_id = get_optional_int(inputs, "__pending_id__", -1)?;
        let matches = st.armed_id.map(|a| a as i64 == incoming_id).unwrap_or(false);
        if !matches || !enabled {
            // stale / disabled → drop + no re-arm
            st.armed_id = None;
            st.last_enabled = enabled;
            return NodeOutput::empty();
        }
        st.tick_count += 1;
        st.last_fired_at = Some(Instant::now());
        // re-arm next
        st.armed_id = Some(self.schedule_next(st, interval_sec, &trigger));
        return NodeOutput::exec("on_tick")
            .with_data("count", SocketValue::Int(st.tick_count))
            .with_data("elapsed_sec", SocketValue::Float(interval_sec));
    }

    // edge detection
    if enabled && st.armed_id.is_none() {
        st.armed_id = Some(self.schedule_next(st, interval_sec, &trigger));
    }
    if !enabled {
        st.armed_id = None; // 次回 stale 判定で自動 drop
    }
    st.last_enabled = enabled;
    NodeOutput::empty()
}

fn schedule_next(&self, st: &mut TimerIntervalState, interval_sec: f32, trigger: &Option<TriggerHandle>) -> u64 {
    let id = st.next_id;
    st.next_id += 1;
    if let Some(t) = trigger {
        let t = t.clone();
        let node_id = /* ctx から取得 */;
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs_f32(interval_sec)).await;
            let _ = t.send(TriggerEvent::new(node_id)
                .with_exec("__tick__")
                .with_override("__pending_id__", SocketValue::Int(id as i64)));
        });
    }
    id
}
```

`DelayNode` との差分は **re-arm ループ**だけ。`node_id` を `StatefulCtx` から取れるかは `ctx.node_fq` 系の
API が無ければ `DelayNode` と同じ経路で対応する。

### 1.5 テスト

- `enabled=true, interval_sec=0.05` で 0.3s 回して tick 数が 4〜7 件（jitter 許容）
- `enabled=false` で 0.3s 回しても `on_tick` が 0 件
- 途中で `enabled=true → false` 切替 → 以降 tick 無し
- `enabled=false → true` 再切替 → 再開して tick 再カウント
- `interval_sec` 動的変更が次 tick から反映

### 1.6 control_triggerable

**false のまま** が妥当。外部から tick を 1 回「叩く」用途は `delay` / 任意の ingress でカバーできる。
タイマー source が外部 API から誤発火すると意図せず副作用ノードが連鎖するため opt-out にしておく。

### 1.7 優先度 / 着手タイミング

- 現時点で blocking な利用者は居ない（Twitch token refresh は現行 processor 側 poll に依存）。
- **φ 完了後の Phase χ は OpenAI Responses API 全面移行に確定**（[`phase-chi-openai-responses.md`](phase-chi-openai-responses.md)）したため、本ノードは **Phase χ 以降の単独ノード PR** として別途扱う。
- もし GUI の Editor Pane live-refresh（e.g. テーブル変更を polling する用途）で内部的に必要になれば
  前倒しも検討。

---
