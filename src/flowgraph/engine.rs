//! VAC Flowgraph 評価エンジン（spec §5）。
//!
//! ## ハイブリッドモデル
//!
//! - **exec DAG**: eager strict。trigger（ingress / self-ingress / 明示発火）から始まり、
//!   `EffectfulNode` / `StatefulNode` / `PureNode`（flow 制御系）が順に発火する。
//! - **data DAG**: pull-demand lazy。副作用ノードが発火したとき、必要な data 入力を
//!   逆向きに `pull_port()` で評価し、結果は generation 内で memoize。
//! - **Generation**: 外部/内部 trigger が発火するたびに `generation += 1`。
//!   Pure のキャッシュは generation ごとにクリア。Stateful の state は保持、
//!   cache key に state version を含めて staleness を防ぐ。

use crate::flowgraph::node::{
 ExecCtx, ExecFireSet, InputMap, NodeExecError, NodeImpl, PortDirection, StatefulCtx, TriggerEvent, TriggerHandle,
};
use crate::flowgraph::socket::{coerce_to_type, SocketType, SocketValue};
use std::collections::{HashMap, HashSet, VecDeque};
use std::future::Future;
use thiserror::Error;
use tokio::sync::mpsc;

pub type NodeId = String;
pub type PortName = String;

/// 外部から `run_forever_with_bus` に渡す trigger bus を生成するヘルパ。
pub fn create_trigger_bus() -> (TriggerHandle, mpsc::UnboundedReceiver<TriggerEvent>) {
 let (tx, rx) = mpsc::unbounded_channel::<TriggerEvent>();
 (TriggerHandle::new(tx), rx)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PortRef {
 pub node: NodeId,
 pub port: PortName,
}

impl PortRef {
 pub fn new(node: impl Into<NodeId>, port: impl Into<PortName>) -> Self {
  Self { node: node.into(), port: port.into() }
 }
}

// ---------------------------------------------------------------------
// NodeInstance
// ---------------------------------------------------------------------

/// 1 ノードインスタンスの「身分」＋ 実行時キャッシュ。
pub struct NodeInstance {
 pub id: NodeId,
 pub impl_: NodeImpl,
 pub properties: InputMap,
 /// (generation, port_name) -> SocketValue のキャッシュ。Pure / Stateful の出力に使う。
 cache: HashMap<(u64, PortName), SocketValue>,
 /// 副作用ノード専用: 最後に exec 発火した generation と出力
 effectful_last_fired: Option<u64>,
 /// Stateful ノードの state version（state が書き換わるたびインクリメント）
 state_version: u64,
}

impl NodeInstance {
 fn new(id: NodeId, impl_: NodeImpl, properties: InputMap) -> Self {
  Self {
   id,
   impl_,
   properties,
   cache: HashMap::new(),
   effectful_last_fired: None,
   state_version: 0,
  }
 }
}

impl std::fmt::Debug for NodeInstance {
 fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
  f.debug_struct("NodeInstance")
   .field("id", &self.id)
   .field("impl_", &self.impl_)
   .field("properties", &self.properties)
   .field("state_version", &self.state_version)
   .finish()
 }
}

#[derive(Debug, Clone)]
pub struct Edge {
 pub from: PortRef,
 pub to: PortRef,
 pub is_exec: bool,
}

// ---------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------

#[derive(Default)]
pub struct FlowgraphBuilder {
 nodes: Vec<NodeInstance>,
 edges: Vec<Edge>,
}

impl FlowgraphBuilder {
 pub fn new() -> Self {
  Self::default()
 }

 pub fn add_node(
  &mut self,
  id: impl Into<NodeId>,
  impl_: NodeImpl,
  properties: InputMap,
 ) -> &mut Self {
  let id = id.into();
  self.nodes.push(NodeInstance::new(id, impl_, properties));
  self
 }

 pub fn connect(&mut self, from: PortRef, to: PortRef) -> &mut Self {
  self.edges.push(Edge { from, to, is_exec: false });
  self
 }

 pub fn connect_exec(&mut self, from: PortRef, to: PortRef) -> &mut Self {
  self.edges.push(Edge { from, to, is_exec: true });
  self
 }

 pub fn build(self) -> Result<FlowgraphProgram, BuildError> {
  build_program(self.nodes, self.edges)
 }
}

// ---------------------------------------------------------------------
// 検証済みプログラム
// ---------------------------------------------------------------------

pub struct FlowgraphProgram {
 nodes: HashMap<NodeId, NodeInstance>,
 /// データ入力ポート → 上流出力ポート（single-source）
 data_sources: HashMap<PortRef, PortRef>,
 /// データ入力ポート（multi=true のみ）→ 上流出力ポート集合
 data_sources_multi: HashMap<PortRef, Vec<PortRef>>,
 /// exec 出力ポート → 下流 exec 入力ポート集合
 exec_succ: HashMap<PortRef, Vec<PortRef>>,
 /// 入力ポートを一切持たないソースノード（初期 exec 発火候補）
 sources: Vec<NodeId>,
 /// 実行統計カウンタ（1-shot `execute()` 毎にリセット）
 generation_seed: u64,
}

impl FlowgraphProgram {
 pub fn node_ids(&self) -> impl Iterator<Item = &NodeId> {
  self.nodes.keys()
 }

 /// 1-shot 実行: 全ソースノードを初期発火して完走させる。event loop は `run_forever`。
 pub async fn execute(&mut self, ctx: &mut ExecCtx) -> Result<ProgramRun, NodeExecError> {
  let mut run = ProgramRun::default();
  self.generation_seed = self.generation_seed.wrapping_add(1);
  let generation = self.generation_seed;
  run.generation = generation;

  let mut queue: VecDeque<(NodeId, ExecFireSet, InputMap)> = VecDeque::new();
  for src in self.sources.clone() {
   queue.push_back((src, ExecFireSet::new(), InputMap::new()));
  }

  while let Some((node_id, fired, overrides)) = queue.pop_front() {
   self.fire_node(&node_id, fired, overrides, generation, ctx, &mut run, &mut queue).await?;
  }

  Ok(run)
 }

 /// 常駐 event loop: 初回 `execute` を 1 パス実行した後、trigger events を待って
 /// 対象ノードを発火し続ける。`shutdown` future が完了するまでブロックする。
 ///
 /// 内部で trigger bus を自動生成する版。Delay 等「engine 内で完結する self-ingress」
 /// だけを扱うテスト向け。外部 Ingress と統合する場合は `run_forever_with_bus` を使う。
 pub async fn run_forever<F>(
  &mut self,
  ctx: &mut ExecCtx,
  shutdown: F,
 ) -> Result<ProgramRun, NodeExecError>
 where
  F: Future<Output = ()>,
 {
  let (handle, rx) = create_trigger_bus();
  self.run_forever_with_bus(ctx, handle, rx, shutdown).await
 }

 /// 外部生成した trigger bus を渡す版。Ingress ノードを外部（HTTP ハンドラ等）から
 /// 叩きたいとき、呼び出し側が `TriggerHandle` のクローンを保持しつつ receiver を
 /// engine に渡すというパターンで使う。
 pub async fn run_forever_with_bus<F>(
  &mut self,
  ctx: &mut ExecCtx,
  handle: TriggerHandle,
  mut rx: mpsc::UnboundedReceiver<TriggerEvent>,
  shutdown: F,
 ) -> Result<ProgramRun, NodeExecError>
 where
  F: Future<Output = ()>,
 {
  ctx.trigger = Some(handle);

  // 初期化パス
  let mut run = self.execute(ctx).await?;

  tokio::pin!(shutdown);

  loop {
   tokio::select! {
    biased;
    _ = &mut shutdown => break,
    event = rx.recv() => {
     match event {
      Some(event) => {
       self.handle_trigger(event, ctx, &mut run).await?;
      }
      None => break,
     }
    }
   }
  }

  ctx.trigger = None;
  Ok(run)
 }

 /// 1 つの `TriggerEvent` を処理する。新しい generation を振り、対象ノードを発火。
 async fn handle_trigger(
  &mut self,
  event: TriggerEvent,
  ctx: &mut ExecCtx,
  run: &mut ProgramRun,
 ) -> Result<(), NodeExecError> {
  if !self.nodes.contains_key(&event.node_id) {
   return Err(NodeExecError::Generic(anyhow::anyhow!(
    "TriggerEvent references unknown node '{}'",
    event.node_id
   )));
  }
  self.generation_seed = self.generation_seed.wrapping_add(1);
  let generation = self.generation_seed;

  let mut fired = ExecFireSet::new();
  for port in event.fired_exec {
   fired.insert(port);
  }

  let mut queue: VecDeque<(NodeId, ExecFireSet, InputMap)> = VecDeque::new();
  queue.push_back((event.node_id, fired, event.data_overrides));

  while let Some((node_id, fired, overrides)) = queue.pop_front() {
   self.fire_node(&node_id, fired, overrides, generation, ctx, run, &mut queue).await?;
  }
  Ok(())
 }

 /// 1 ノードを「exec 発火」する。data 入力は pull で評価される。
 /// `data_overrides` は「この 1 回の発火に限って」pull より優先される値のマップ。
 /// `TriggerEvent::data_overrides` の伝搬に使う（Delay の self-trigger が典型例）。
 #[allow(clippy::too_many_arguments)]
 async fn fire_node(
  &mut self,
  node_id: &NodeId,
  fired_exec: ExecFireSet,
  data_overrides: InputMap,
  generation: u64,
  ctx: &mut ExecCtx,
  run: &mut ProgramRun,
  queue: &mut VecDeque<(NodeId, ExecFireSet, InputMap)>,
 ) -> Result<(), NodeExecError> {
  let spec = self.nodes[node_id].impl_.describe();

  // data 入力を準備: overrides 優先、なければ pull
  let mut inputs: InputMap = HashMap::new();
  for port in spec.inputs.iter() {
   if port.is_exec {
    continue;
   }
   if let Some(v) = data_overrides.get(&port.name) {
    let coerced = coerce_input(node_id, &port.name, &port.ty, v.clone())?;
    inputs.insert(port.name.clone(), coerced);
    continue;
   }
   match self.pull_input(node_id, &port.name, generation, ctx, run).await? {
    Some(v) => {
     let coerced = coerce_input(node_id, &port.name, &port.ty, v)?;
     inputs.insert(port.name.clone(), coerced);
    }
    None => {
     if !port.optional && port.default.is_none() {
      return Err(NodeExecError::MissingRequiredInput(format!("{}:{}", node_id, port.name)));
     }
     if let Some(def) = port.default.as_ref() {
      let v = def
       .to_socket_value(&port.ty)
       .ok_or_else(|| NodeExecError::MissingRequiredInput(format!("{}:{}", node_id, port.name)))?;
      inputs.insert(port.name.clone(), v);
     }
    }
   }
  }

  // 評価: 種別ごとに分岐
  let trigger_handle_opt = ctx.trigger.clone();
  let prev_node_id = std::mem::replace(&mut ctx.node_id, node_id.clone());
  let node = self.nodes.get_mut(node_id).expect("node exists");
  let props = node.properties.clone();
  let result = match &mut node.impl_ {
   NodeImpl::Pure(pn) => pn.compute(&props, &inputs, &fired_exec).await?,
   NodeImpl::Stateful { node: sn, state } => {
    let sctx = StatefulCtx { node_id, trigger: trigger_handle_opt.as_ref() };
    let before = node.state_version;
    let out = sn.compute(state.as_mut(), &props, &inputs, &fired_exec, &sctx).await?;
    node.state_version = before.wrapping_add(1);
    out
   }
   NodeImpl::Effectful(en) => en.execute(ctx, &props, &inputs, &fired_exec).await?,
  };
  ctx.node_id = prev_node_id;

  // 統計
  run.exec_count.entry(node_id.clone()).and_modify(|n| *n += 1).or_insert(1);

  // 出力を cache
  let node = self.nodes.get_mut(node_id).unwrap();
  for (port, value) in result.data.iter() {
   node.cache.insert((generation, port.clone()), value.clone());
  }
  if node.impl_.is_effectful() {
   node.effectful_last_fired = Some(generation);
  }

  // 診断用: 全出力値を run に蓄積（memoize ではなく、テスト可視性のため）
  for (port, value) in result.data.iter() {
   run.stored_values.insert(PortRef::new(node_id, port), value.clone());
  }

  // exec 出力を下流に配送（下流は overrides なし）
  for fired_port in result.fired_exec.iter() {
   let src = PortRef::new(node_id, fired_port);
   if let Some(dst_list) = self.exec_succ.get(&src) {
    for dst in dst_list {
     let mut set = ExecFireSet::new();
     set.insert(dst.port.clone());
     queue.push_back((dst.node.clone(), set, InputMap::new()));
    }
   }
  }

  Ok(())
 }

 /// `(dst_node, dst_port)` の入力値を pull で評価。
 ///
 /// - 上流ノードが Pure/Stateful: `pull_node_output()` で再帰評価し cache から返す
 /// - 上流ノードが Effectful: 発火済みで cache に値があれば返す、未発火ならエラー
 /// - 接続がなければ `None`（default / optional 判定は呼び出し側で行う）
 async fn pull_input(
  &mut self,
  dst_node: &NodeId,
  dst_port: &str,
  generation: u64,
  ctx: &mut ExecCtx,
  run: &mut ProgramRun,
 ) -> Result<Option<SocketValue>, NodeExecError> {
  let pref = PortRef::new(dst_node, dst_port);
  // multi=true の場合（未実装、δ-2 以降で追加）
  if let Some(list) = self.data_sources_multi.get(&pref).cloned() {
   let mut out = Vec::with_capacity(list.len());
   for src in list {
    out.push(self.pull_node_output(&src.node, &src.port, generation, ctx, run).await?);
   }
   return Ok(Some(SocketValue::List(out)));
  }
  // single source
  if let Some(src) = self.data_sources.get(&pref).cloned() {
   return Ok(Some(self.pull_node_output(&src.node, &src.port, generation, ctx, run).await?));
  }
  Ok(None)
 }

 /// 上流ノードの出力ポートの値を pull（demand lazy）。
 /// キャッシュがあれば即返す、なければ評価する。
 async fn pull_node_output(
  &mut self,
  src_node: &NodeId,
  src_port: &str,
  generation: u64,
  ctx: &mut ExecCtx,
  run: &mut ProgramRun,
 ) -> Result<SocketValue, NodeExecError> {
  // 1. cache ヒット?
  if let Some(v) = self.nodes[src_node].cache.get(&(generation, src_port.to_string())) {
   run.cache_hits += 1;
   return Ok(v.clone());
  }
  run.cache_misses += 1;

  // 2. ノード種別で分岐
  match &self.nodes[src_node].impl_ {
   NodeImpl::Effectful(_) => {
    // 直近発火で値があれば返す（世代またぎでも last fire を返す、spec 通り）
    if let Some(v) = self.nodes[src_node]
     .cache
     .iter()
     .find_map(|((_g, p), v)| if p == src_port { Some(v.clone()) } else { None })
    {
     Ok(v)
    } else {
     Err(NodeExecError::EffectfulPulledBeforeFiring(format!("{src_node}:{src_port}")))
    }
   }
   NodeImpl::Pure(_) | NodeImpl::Stateful { .. } => {
    // 3. 再帰的に入力を pull して評価
    self.evaluate_pure_or_stateful(src_node, generation, ctx, run).await?;
    // 4. 再度 cache からポート値を取得
    self.nodes[src_node]
     .cache
     .get(&(generation, src_port.to_string()))
     .cloned()
     .ok_or_else(|| NodeExecError::MissingRequiredInput(format!("{src_node}:{src_port} (no output after compute)")))
   }
  }
 }

 /// Pure / Stateful ノードを pull コンテキストで評価。exec 発火は無効（data だけ取得）。
 async fn evaluate_pure_or_stateful(
  &mut self,
  node_id: &NodeId,
  generation: u64,
  ctx: &mut ExecCtx,
  run: &mut ProgramRun,
 ) -> Result<(), NodeExecError> {
  let spec = self.nodes[node_id].impl_.describe();
  let mut inputs: InputMap = HashMap::new();
  for port in spec.inputs.iter() {
   if port.is_exec {
    continue;
   }
   match Box::pin(self.pull_input(node_id, &port.name, generation, ctx, run)).await? {
    Some(v) => {
     let coerced = coerce_input(node_id, &port.name, &port.ty, v)?;
     inputs.insert(port.name.clone(), coerced);
    }
    None => {
     if !port.optional && port.default.is_none() {
      return Err(NodeExecError::MissingRequiredInput(format!("{}:{}", node_id, port.name)));
     }
     if let Some(def) = port.default.as_ref() {
      let v = def
       .to_socket_value(&port.ty)
       .ok_or_else(|| NodeExecError::MissingRequiredInput(format!("{}:{}", node_id, port.name)))?;
      inputs.insert(port.name.clone(), v);
     }
    }
   }
  }

  let trigger_handle_opt = ctx.trigger.clone();
  let node = self.nodes.get_mut(node_id).unwrap();
  let props = node.properties.clone();
  // pull context では exec 発火 set は空（exec 発火は別経路）
  let empty_fired = ExecFireSet::new();
  let result = match &mut node.impl_ {
   NodeImpl::Pure(pn) => pn.compute(&props, &inputs, &empty_fired).await?,
   NodeImpl::Stateful { node: sn, state } => {
    let sctx = StatefulCtx { node_id, trigger: trigger_handle_opt.as_ref() };
    let out = sn.compute(state.as_mut(), &props, &inputs, &empty_fired, &sctx).await?;
    node.state_version = node.state_version.wrapping_add(1);
    out
   }
   NodeImpl::Effectful(_) => unreachable!("effectful handled in pull_node_output"),
  };

  run.pure_evaluations.entry(node_id.clone()).and_modify(|n| *n += 1).or_insert(1);

  // 全 data 出力を cache
  let node = self.nodes.get_mut(node_id).unwrap();
  for (port, value) in result.data.iter() {
   node.cache.insert((generation, port.clone()), value.clone());
  }
  for (port, value) in result.data.iter() {
   run.stored_values.insert(PortRef::new(node_id, port), value.clone());
  }
  Ok(())
 }
}

// ---------------------------------------------------------------------
// 実行結果
// ---------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct ProgramRun {
 pub generation: u64,
 /// 診断用: 評価された値のスナップショット
 pub stored_values: HashMap<PortRef, SocketValue>,
 /// ノード毎の exec 発火回数
 pub exec_count: HashMap<NodeId, usize>,
 /// Pure/Stateful の pull 評価回数（メモ化効果の可視化）
 pub pure_evaluations: HashMap<NodeId, usize>,
 pub cache_hits: usize,
 pub cache_misses: usize,
}

impl ProgramRun {
 pub fn value_at(&self, node: &str, port: &str) -> Option<&SocketValue> {
  self.stored_values.get(&PortRef::new(node, port))
 }
 pub fn exec_count_of(&self, node: &str) -> usize {
  self.exec_count.get(node).copied().unwrap_or(0)
 }
 pub fn pure_evaluations_of(&self, node: &str) -> usize {
  self.pure_evaluations.get(node).copied().unwrap_or(0)
 }
}

// ---------------------------------------------------------------------
// 構築時検証
// ---------------------------------------------------------------------

fn build_program(raw_nodes: Vec<NodeInstance>, raw_edges: Vec<Edge>) -> Result<FlowgraphProgram, BuildError> {
 let mut nodes: HashMap<NodeId, NodeInstance> = HashMap::new();
 for n in raw_nodes {
  if nodes.contains_key(&n.id) {
   return Err(BuildError::DuplicateNodeId(n.id));
  }
  nodes.insert(n.id.clone(), n);
 }

 for e in &raw_edges {
  let from_node = nodes.get(&e.from.node).ok_or_else(|| BuildError::UnknownNode(e.from.node.clone()))?;
  let to_node = nodes.get(&e.to.node).ok_or_else(|| BuildError::UnknownNode(e.to.node.clone()))?;
  let from_spec = from_node.impl_.describe();
  let to_spec = to_node.impl_.describe();
  let fp = from_spec
   .find_output(&e.from.port)
   .ok_or_else(|| BuildError::UnknownPort(e.from.clone()))?;
  let tp = to_spec
   .find_input(&e.to.port)
   .ok_or_else(|| BuildError::UnknownPort(e.to.clone()))?;
  if fp.direction != PortDirection::Output {
   return Err(BuildError::WrongDirection(e.from.clone()));
  }
  if tp.direction != PortDirection::Input {
   return Err(BuildError::WrongDirection(e.to.clone()));
  }
  if fp.is_exec != tp.is_exec {
   return Err(BuildError::ExecDataMixed(Box::new(ExecDataMixedDetail {
    from: e.from.clone(),
    to: e.to.clone(),
   })));
  }
  if !fp.ty.compatible_with(&tp.ty) {
   return Err(BuildError::TypeMismatch(Box::new(TypeMismatchDetail {
    from: e.from.clone(),
    from_ty: fp.ty.clone(),
    to: e.to.clone(),
    to_ty: tp.ty.clone(),
   })));
  }
 }

 let mut data_sources: HashMap<PortRef, PortRef> = HashMap::new();
 let mut data_sources_multi: HashMap<PortRef, Vec<PortRef>> = HashMap::new();
 let mut exec_succ: HashMap<PortRef, Vec<PortRef>> = HashMap::new();
 let mut data_fan_in: HashMap<PortRef, usize> = HashMap::new();

 for e in raw_edges {
  let from_node = nodes.get(&e.from.node).unwrap();
  let from_spec = from_node.impl_.describe();
  let fp = from_spec.find_output(&e.from.port).unwrap();
  if fp.ty == SocketType::Exec {
   exec_succ.entry(e.from.clone()).or_default().push(e.to.clone());
   continue;
  }
  // data
  let to_node = nodes.get(&e.to.node).unwrap();
  let to_spec = to_node.impl_.describe();
  let tp = to_spec.find_input(&e.to.port).unwrap();
  *data_fan_in.entry(e.to.clone()).or_insert(0) += 1;
  if !tp.multi && data_fan_in[&e.to] > 1 {
   return Err(BuildError::FanInOnNonMulti(e.to.clone()));
  }
  if tp.multi {
   data_sources_multi.entry(e.to.clone()).or_default().push(e.from.clone());
  } else {
   data_sources.insert(e.to.clone(), e.from.clone());
  }
 }

 detect_cycles_data(&nodes, &data_sources, &data_sources_multi)?;
 detect_cycles_exec(&nodes, &exec_succ)?;

 // ソース判定（spec §5.2）
 //
 // - Effectful / Stateful で inputs 空 → exec 発火の起点（外部 trigger なしでも初期発火）
 // - Pure で inputs 空 かつ outputs に exec を含む → exec ソース（Sequence 等）
 // - Pure で data のみ出力 → 初期発火しない（pull されて初めて評価、lazy）
 let mut sources: Vec<NodeId> = Vec::new();
 for (id, instance) in &nodes {
  let spec = instance.impl_.describe();
  if !spec.inputs.is_empty() {
   continue;
  }
  let effectful_or_stateful = matches!(&instance.impl_, NodeImpl::Effectful(_) | NodeImpl::Stateful { .. });
  let pure_exec_source = instance.impl_.is_pure() && spec.outputs.iter().any(|p| p.is_exec);
  if effectful_or_stateful || pure_exec_source {
   sources.push(id.clone());
  }
 }
 sources.sort();

 Ok(FlowgraphProgram {
  nodes,
  data_sources,
  data_sources_multi,
  exec_succ,
  sources,
  generation_seed: 0,
 })
}

fn detect_cycles_data(
 nodes: &HashMap<NodeId, NodeInstance>,
 data_sources: &HashMap<PortRef, PortRef>,
 data_sources_multi: &HashMap<PortRef, Vec<PortRef>>,
) -> Result<(), BuildError> {
 let mut preds: HashMap<NodeId, HashSet<NodeId>> = HashMap::new();
 for (dst, src) in data_sources {
  preds.entry(dst.node.clone()).or_default().insert(src.node.clone());
 }
 for (dst, srcs) in data_sources_multi {
  for src in srcs {
   preds.entry(dst.node.clone()).or_default().insert(src.node.clone());
  }
 }
 dfs_detect_cycle(nodes, &preds, "data")
}

fn detect_cycles_exec(
 nodes: &HashMap<NodeId, NodeInstance>,
 exec_succ: &HashMap<PortRef, Vec<PortRef>>,
) -> Result<(), BuildError> {
 let mut preds: HashMap<NodeId, HashSet<NodeId>> = HashMap::new();
 for (src, dsts) in exec_succ {
  for dst in dsts {
   preds.entry(dst.node.clone()).or_default().insert(src.node.clone());
  }
 }
 dfs_detect_cycle(nodes, &preds, "exec")
}

fn dfs_detect_cycle(
 nodes: &HashMap<NodeId, NodeInstance>,
 preds: &HashMap<NodeId, HashSet<NodeId>>,
 kind: &'static str,
) -> Result<(), BuildError> {
 let mut visited: HashSet<NodeId> = HashSet::new();
 let mut stack: HashSet<NodeId> = HashSet::new();
 for id in nodes.keys() {
  if !visited.contains(id) && dfs_cycle(id, preds, &mut visited, &mut stack) {
   return Err(BuildError::CycleDetected(kind.into()));
  }
 }
 Ok(())
}

fn dfs_cycle(
 cur: &NodeId,
 preds: &HashMap<NodeId, HashSet<NodeId>>,
 visited: &mut HashSet<NodeId>,
 stack: &mut HashSet<NodeId>,
) -> bool {
 if stack.contains(cur) {
  return true;
 }
 if visited.contains(cur) {
  return false;
 }
 stack.insert(cur.clone());
 if let Some(pre) = preds.get(cur) {
  for p in pre {
   if dfs_cycle(p, preds, visited, stack) {
    return true;
   }
  }
 }
 stack.remove(cur);
 visited.insert(cur.clone());
 false
}

// ---------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum BuildError {
 #[error("ノード ID 重複: {0}")]
 DuplicateNodeId(NodeId),
 #[error("未登録ノードへの参照: {0}")]
 UnknownNode(NodeId),
 #[error("未知ポート: {0:?}")]
 UnknownPort(PortRef),
 #[error("ポート方向不整合: {0:?}")]
 WrongDirection(PortRef),
 #[error("エッジが exec と data を跨いでいる: {from:?} -> {to:?}", from = _0.from, to = _0.to)]
 ExecDataMixed(Box<ExecDataMixedDetail>),
 #[error("エッジの型不一致: {from:?}({from_ty}) -> {to:?}({to_ty})", from = _0.from, from_ty = _0.from_ty, to = _0.to, to_ty = _0.to_ty)]
 TypeMismatch(Box<TypeMismatchDetail>),
 #[error("single-input ポートに複数入力: {0:?}")]
 FanInOnNonMulti(PortRef),
 #[error("{0} DAG に循環を検出")]
 CycleDetected(String),
}

#[derive(Debug)]
pub struct TypeMismatchDetail {
 pub from: PortRef,
 pub from_ty: SocketType,
 pub to: PortRef,
 pub to_ty: SocketType,
}

#[derive(Debug)]
pub struct ExecDataMixedDetail {
 pub from: PortRef,
 pub to: PortRef,
}

/// 入力値をポート型に合わせて暗黙 coerce（Float ↔ Quantity 等）。
///
/// Phase ξ-3 で追加。エッジ接続時の型互換は [`SocketType::compatible_with`] で許容
/// しているため、ランタイム配送の直前に値を target port 型へ揃える必要がある。
fn coerce_input(
	node_id: &NodeId,
	port_name: &str,
	target_ty: &SocketType,
	value: SocketValue,
) -> Result<SocketValue, NodeExecError> {
	coerce_to_type(value, target_ty).map_err(|e| {
		NodeExecError::Generic(anyhow::anyhow!(
			"ノード '{}' の入力 '{}' の coerce 失敗: {}",
			node_id,
			port_name,
			e
		))
	})
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
 use super::*;
 use crate::flowgraph::node::{InputMap, NodeDescriptor, NodeOutput, NodeSpec, PortSpec, PureNode};
 use crate::flowgraph::nodes::flow::{BranchNode, SequenceNode};
 use crate::flowgraph::nodes::literal::{BoolLiteralNode, IntLiteralNode, StringLiteralNode};
 use crate::flowgraph::nodes::log::LogNode;
 use crate::flowgraph::socket::SocketValue;
 use async_trait::async_trait;
 use std::sync::atomic::{AtomicUsize, Ordering};
 use std::sync::Arc;

 fn assert_build_err<T, E: std::fmt::Debug>(r: Result<T, E>) -> E {
  match r {
   Ok(_) => panic!("expected Err"),
   Err(e) => e,
  }
 }

 // ----- 構築時検証テスト -------------------------------------------------

 #[test]
 fn build_detects_duplicate_id() {
  let mut b = FlowgraphBuilder::new();
  b.add_node("n", NodeImpl::pure(Arc::new(StringLiteralNode)), InputMap::new());
  b.add_node("n", NodeImpl::pure(Arc::new(StringLiteralNode)), InputMap::new());
  let e = assert_build_err(b.build());
  assert!(matches!(e, BuildError::DuplicateNodeId(_)), "{e:?}");
 }

 #[test]
 fn build_detects_unknown_node_ref() {
  let mut b = FlowgraphBuilder::new();
  b.add_node("a", NodeImpl::pure(Arc::new(StringLiteralNode)), InputMap::new());
  b.connect(PortRef::new("a", "value"), PortRef::new("missing", "value"));
  let e = assert_build_err(b.build());
  assert!(matches!(e, BuildError::UnknownNode(_)), "{e:?}");
 }

 #[test]
 fn build_detects_unknown_port() {
  let mut b = FlowgraphBuilder::new();
  b.add_node("a", NodeImpl::pure(Arc::new(StringLiteralNode)), InputMap::new());
  b.add_node("b", NodeImpl::effectful(Arc::new(LogNode)), InputMap::new());
  b.connect(PortRef::new("a", "no_such_port"), PortRef::new("b", "value"));
  let e = assert_build_err(b.build());
  assert!(matches!(e, BuildError::UnknownPort(_)), "{e:?}");
 }

 #[test]
 fn build_detects_type_mismatch() {
  let mut b = FlowgraphBuilder::new();
  b.add_node("i", NodeImpl::pure(Arc::new(IntLiteralNode)), InputMap::new());
  b.add_node("log", NodeImpl::effectful(Arc::new(LogNode)), InputMap::new());
  b.connect(PortRef::new("i", "value"), PortRef::new("log", "value"));
  let e = assert_build_err(b.build());
  assert!(matches!(e, BuildError::TypeMismatch(_)), "{e:?}");
 }

 #[test]
 fn build_detects_exec_data_mixed() {
  let mut b = FlowgraphBuilder::new();
  b.add_node("seq", NodeImpl::pure(Arc::new(SequenceNode::new(1))), InputMap::new());
  b.add_node("log", NodeImpl::effectful(Arc::new(LogNode)), InputMap::new());
  // 通常エッジに exec 出力を繋ぐ → exec/data 混合
  b.connect(PortRef::new("seq", "exec_1"), PortRef::new("log", "value"));
  let e = assert_build_err(b.build());
  assert!(matches!(e, BuildError::TypeMismatch(_) | BuildError::ExecDataMixed(_)), "{e:?}");
 }

 // ----- 基本実行テスト ---------------------------------------------------

 #[tokio::test]
 async fn sequence_fires_log_in_order() {
  // Sequence(3) → Log ×3
  let mut b = FlowgraphBuilder::new();
  b.add_node("seq", NodeImpl::pure(Arc::new(SequenceNode::new(3))), InputMap::new());
  for i in 1..=3 {
   let id = format!("log{i}");
   b.add_node(&id, NodeImpl::effectful(Arc::new(LogNode)), InputMap::new());
   b.add_node(
    &format!("msg{i}"),
    NodeImpl::pure(Arc::new(StringLiteralNode)),
    [("value".into(), SocketValue::String(format!("#{i}")))].into_iter().collect(),
   );
   b.connect_exec(PortRef::new("seq", format!("exec_{i}")), PortRef::new(&id, "exec_in"));
   b.connect(PortRef::new(&format!("msg{i}"), "value"), PortRef::new(&id, "value"));
  }
  let mut prog = b.build().expect("build");
  let mut ctx = ExecCtx::default();
  let _run = prog.execute(&mut ctx).await.expect("execute");
  assert_eq!(ctx.trace.len(), 3);
  assert!(ctx.trace[0].contains("#1"));
  assert!(ctx.trace[1].contains("#2"));
  assert!(ctx.trace[2].contains("#3"));
 }

 /// Phase \u{3be}-4: Quantity 出力を String ポート（LogNode.value）へ繋ぐと、
 /// engine レベルの暗黙 coerce が `"{value} {unit}"` に自動変換して trace に流す。
 #[tokio::test]
 async fn log_receives_quantity_as_formatted_string() {
  use crate::flowgraph::nodes::literal::FloatLiteralNode;
  use crate::flowgraph::nodes::unit::UnitAssignNode;
  let mut b = FlowgraphBuilder::new();
  b.add_node("seq", NodeImpl::pure(Arc::new(SequenceNode::new(1))), InputMap::new());
  b.add_node("log", NodeImpl::effectful(Arc::new(LogNode)), InputMap::new());
  b.add_node(
   "speed",
   NodeImpl::pure(Arc::new(FloatLiteralNode)),
   [("value".into(), SocketValue::Float(9.81))].into_iter().collect(),
  );
  b.add_node(
   "assign",
   NodeImpl::pure(Arc::new(UnitAssignNode)),
   [("unit".into(), SocketValue::String("m/s^2".into()))].into_iter().collect(),
  );
  b.connect_exec(PortRef::new("seq", "exec_1"), PortRef::new("log", "exec_in"));
  b.connect(PortRef::new("speed", "value"), PortRef::new("assign", "value"));
  b.connect(PortRef::new("assign", "result"), PortRef::new("log", "value"));
  let mut prog = b.build().unwrap();
  let mut ctx = ExecCtx::default();
  let _run = prog.execute(&mut ctx).await.unwrap();
  assert_eq!(ctx.trace.len(), 1, "exactly one log line");
  let line = &ctx.trace[0];
  assert!(line.contains("9.81"), "value present: {line}");
  assert!(line.contains("m") && line.contains("s"), "unit (m/s^2 canonical form) present: {line}");
 }

 #[tokio::test]
 async fn string_literal_pulls_lazily_for_log() {
  let mut b = FlowgraphBuilder::new();
  b.add_node("seq", NodeImpl::pure(Arc::new(SequenceNode::new(1))), InputMap::new());
  b.add_node("log", NodeImpl::effectful(Arc::new(LogNode)), InputMap::new());
  b.add_node(
   "msg",
   NodeImpl::pure(Arc::new(StringLiteralNode)),
   [("value".into(), SocketValue::String("lazy-hi".into()))].into_iter().collect(),
  );
  b.connect_exec(PortRef::new("seq", "exec_1"), PortRef::new("log", "exec_in"));
  b.connect(PortRef::new("msg", "value"), PortRef::new("log", "value"));
  let mut prog = b.build().unwrap();
  let mut ctx = ExecCtx::default();
  let run = prog.execute(&mut ctx).await.unwrap();
  assert!(ctx.trace[0].contains("lazy-hi"));
  assert_eq!(run.pure_evaluations_of("msg"), 1);
  assert_eq!(run.pure_evaluations_of("seq"), 0, "source は exec 発火、pull 評価でない");
 }

 // ----- Branch の dead-port elimination ---------------------------------

 /// 評価回数を数える計測用 pure ノード（String を渡すだけ）。
 struct CountingString(Arc<AtomicUsize>, String);
 impl NodeDescriptor for CountingString {
  fn describe(&self) -> NodeSpec {
   NodeSpec {
    feature: "test.counting_string".into(),
    title: "CountingString".into(),
    category: "test".into(),
    description: None,
    inputs: vec![],
    outputs: vec![PortSpec::output("value", "Value", SocketType::String)],
    properties: vec![],
   }
  }
 }
 #[async_trait]
 impl PureNode for CountingString {
  async fn compute(
   &self,
   _p: &InputMap,
   _i: &InputMap,
   _f: &ExecFireSet,
  ) -> Result<NodeOutput, NodeExecError> {
   self.0.fetch_add(1, Ordering::SeqCst);
   Ok(NodeOutput::new().set_data("value", SocketValue::String(self.1.clone())))
  }
 }

 #[tokio::test]
 async fn branch_does_not_evaluate_dead_then_branch() {
  // Seq -> Branch(cond=false) -> then: Log(counter_then) / else: Log(counter_else)
  let then_count = Arc::new(AtomicUsize::new(0));
  let else_count = Arc::new(AtomicUsize::new(0));

  let mut b = FlowgraphBuilder::new();
  b.add_node("seq", NodeImpl::pure(Arc::new(SequenceNode::new(1))), InputMap::new());
  b.add_node(
   "cond",
   NodeImpl::pure(Arc::new(BoolLiteralNode)),
   [("value".into(), SocketValue::Bool(false))].into_iter().collect(),
  );
  b.add_node("branch", NodeImpl::pure(Arc::new(BranchNode)), InputMap::new());
  b.add_node("log_then", NodeImpl::effectful(Arc::new(LogNode)), InputMap::new());
  b.add_node("log_else", NodeImpl::effectful(Arc::new(LogNode)), InputMap::new());
  b.add_node(
   "msg_then",
   NodeImpl::pure(Arc::new(CountingString(then_count.clone(), "then-side".into()))),
   InputMap::new(),
  );
  b.add_node(
   "msg_else",
   NodeImpl::pure(Arc::new(CountingString(else_count.clone(), "else-side".into()))),
   InputMap::new(),
  );
  b.connect_exec(PortRef::new("seq", "exec_1"), PortRef::new("branch", "exec_in"));
  b.connect(PortRef::new("cond", "value"), PortRef::new("branch", "cond"));
  b.connect_exec(PortRef::new("branch", "then"), PortRef::new("log_then", "exec_in"));
  b.connect_exec(PortRef::new("branch", "else"), PortRef::new("log_else", "exec_in"));
  b.connect(PortRef::new("msg_then", "value"), PortRef::new("log_then", "value"));
  b.connect(PortRef::new("msg_else", "value"), PortRef::new("log_else", "value"));

  let mut prog = b.build().unwrap();
  let mut ctx = ExecCtx::default();
  let _ = prog.execute(&mut ctx).await.unwrap();

  // else 側のみ実行された
  assert_eq!(ctx.trace.len(), 1);
  assert!(ctx.trace[0].contains("else-side"));
  // 死んだ then 側の msg_then は pull されない → 評価回数 0
  assert_eq!(then_count.load(Ordering::SeqCst), 0, "dead port が評価されている");
  assert_eq!(else_count.load(Ordering::SeqCst), 1);
 }

 // ----- メモ化テスト ----------------------------------------------------

 #[tokio::test]
 async fn pure_node_memoized_within_one_generation() {
  // 1 つの pure ノードを 2 つの Log が共有
  let count = Arc::new(AtomicUsize::new(0));
  let mut b = FlowgraphBuilder::new();
  b.add_node("seq", NodeImpl::pure(Arc::new(SequenceNode::new(2))), InputMap::new());
  b.add_node(
   "msg",
   NodeImpl::pure(Arc::new(CountingString(count.clone(), "shared".into()))),
   InputMap::new(),
  );
  b.add_node("log1", NodeImpl::effectful(Arc::new(LogNode)), InputMap::new());
  b.add_node("log2", NodeImpl::effectful(Arc::new(LogNode)), InputMap::new());
  b.connect_exec(PortRef::new("seq", "exec_1"), PortRef::new("log1", "exec_in"));
  b.connect_exec(PortRef::new("seq", "exec_2"), PortRef::new("log2", "exec_in"));
  b.connect(PortRef::new("msg", "value"), PortRef::new("log1", "value"));
  b.connect(PortRef::new("msg", "value"), PortRef::new("log2", "value"));

  let mut prog = b.build().unwrap();
  let mut ctx = ExecCtx::default();
  let run = prog.execute(&mut ctx).await.unwrap();
  assert_eq!(ctx.trace.len(), 2);
  // 2 回 pull されたが評価は 1 回
  assert_eq!(count.load(Ordering::SeqCst), 1);
  assert_eq!(run.pure_evaluations_of("msg"), 1);
  assert!(run.cache_hits >= 1);
 }

 #[tokio::test]
 async fn generation_separates_caches() {
  // 同じ program を 2 回 execute して、各 generation で再評価されることを確認
  let count = Arc::new(AtomicUsize::new(0));
  let mut b = FlowgraphBuilder::new();
  b.add_node("seq", NodeImpl::pure(Arc::new(SequenceNode::new(1))), InputMap::new());
  b.add_node(
   "msg",
   NodeImpl::pure(Arc::new(CountingString(count.clone(), "gen".into()))),
   InputMap::new(),
  );
  b.add_node("log", NodeImpl::effectful(Arc::new(LogNode)), InputMap::new());
  b.connect_exec(PortRef::new("seq", "exec_1"), PortRef::new("log", "exec_in"));
  b.connect(PortRef::new("msg", "value"), PortRef::new("log", "value"));

  let mut prog = b.build().unwrap();
  let mut ctx = ExecCtx::default();
  let r1 = prog.execute(&mut ctx).await.unwrap();
  let r2 = prog.execute(&mut ctx).await.unwrap();
  assert_ne!(r1.generation, r2.generation);
  // 2 回の generation で計 2 回評価
  assert_eq!(count.load(Ordering::SeqCst), 2);
 }
}

