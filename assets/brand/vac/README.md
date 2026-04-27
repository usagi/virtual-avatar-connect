# VAC Artwork Assets

このディレクトリは Virtual Avatar Connect の公式アートワーク素材置き場です。

## ディレクトリ

- `design-master/`
  - デザインマスターを置く場所です。
  - 意図してデザインマスターそのものを編集する場合を除き、直接編集しないでください。
  - `virtual-avatar-connect-circle.png` はアイコン / システムトレイ向けの現在の採用版です。
  - `virtual-avatar-connect-roundsq.png` は正角丸版の現在の採用版です。
  - 加工、縮小、色相回転、アイコン化、トレイ用最適化は `derived/` へ派生ファイルとして出力します。
- `derived/`
  - アプリ組み込み用の派生素材を置く場所です。
  - `icon.ico`、`favicon.ico`、tray icon などはここから生成・同期します。

## 運用ルール

- デザインマスターは「見た目の正本」です。誤って上書きしないよう、作業前に目的を確認してください。
- 派生素材は用途別に作ります。小さいサイズでは線や光を減らし、シルエットと外周 halo を優先します。
- 色相回転などの mode / theme variant は、デザインマスターを直接変更せず、派生素材として追加します。
- root の `icon.ico` と `favicon.ico` はアプリ組み込み互換のために残します。正本はこのディレクトリです。
