# Flowgraph Fixture Testing

Virtual Avatar Connect の Flowgraph fixture runner は、Flowgraph を headless に実行するための開発者向けテスト機構です。
外部 HTTP / ファイル IO を mock し、trace と recorded effects を検証することで、Flowgraph 言語コアと標準ノードの回帰を小さく確認できます。

GUI E2E が「画面と Control API の往復」を見るのに対し、Flowgraph fixture は「Flowgraph そのものの実行結果」を見る層です。
OpenAI / Twitch / OBS / TTS などの実サービスを叩かず、`flowgraph.example/` 配下だけで再現可能な fixture を増やしていく方針。

## 単体 fixture の実行

`--flowgraph-test-dir` には、`*.flowgraph.test.toml` を含むディレクトリを指定します。
テスト定義が 1 件も見つからない場合は、誤ったディレクトリを成功扱いしないため失敗します。

```powershell
cargo run --bin virtual-avatar-connect-cli -- --flowgraph-test-dir flowgraph.example/table-write-tsv-mock
```

機械処理や差分確認には JSON 出力を使います。

```powershell
cargo run --bin virtual-avatar-connect-cli -- --flowgraph-test-dir flowgraph.example/table-write-tsv-mock --flowgraph-test-json
```

## suite 実行

`--flowgraph-test-root` は、指定 root 以下から `*.flowgraph.test.toml` を含む fixture ディレクトリを探索してまとめて実行します。
fixture ディレクトリを見つけたらそこで探索を止めるため、fixture 内部の補助ファイルは別 fixture として扱われません。
fixture が 1 件も見つからない root は失敗扱いです。

```powershell
cargo run --bin virtual-avatar-connect-cli -- --flowgraph-test-root flowgraph.example
```

JSON 出力では suite 全体の成否、fixture 件数、失敗件数、各 fixture の report を返します。

```powershell
cargo run --bin virtual-avatar-connect-cli -- --flowgraph-test-root flowgraph.example --flowgraph-test-json
```

`--flowgraph-test-dir` と `--flowgraph-test-root` は同時指定できません。

## fixture ファイル構成

fixture ディレクトリには、通常の `*.flowgraph.toml` とテスト定義 `*.flowgraph.test.toml` を置きます。
runner は指定ディレクトリ内の Flowgraph をまとめてロードするため、テスト定義側に対象 Flowgraph ファイル名は書きません。
テスト定義の最小構成は次の形です。

```toml
[tests.expect]
node_count = 2
trigger_count = 1
trace_count = 2
effect_count = 1

[[triggers]]
node = "ingress"

[[triggers.overrides]]
port = "text"
value = "hello"
```

`[tests.expect]` には実行結果の件数や trace を書けます。

```toml
[tests.expect]
node_count = 3
mock_count = 1
trigger_count = 1
trace_count = 3
effect_count = 1
trace = [
  "ingress -> transform",
  "transform -> out",
]

[[tests.expect.exec_count]]
node = "out"
count = 1

[[tests.expect.stored_values]]
node = "result"
port = "value"
value = "expected"
```

投入した trigger は `trigger_history[]` として記録され、node / exec / delay / override を検証できます。

```toml
[[tests.expect.trigger_history]]
node = "ingress"
exec = ["__trigger__"]
delay_ms = 0

[[tests.expect.trigger_history.overrides]]
port = "text"
ty = "string"
value = "hello"
```

## mock IO

fixture runner は現時点で HTTP、ファイル読み込み、ファイル書き込みを mock できます。
mock は node id 単位で対応付けます。

```toml
[[mocks.http]]
node = "request"
status = 200
json = { ok = true, message = "mocked" }

[[mocks.file_read]]
node = "load_table"
contents = "name\tscore\namiya\t100\n"

[[mocks.file_write]]
node = "write_table"
```

失敗 branch の検証には `error` を使います。

```toml
[[mocks.http]]
node = "request"
error = "mock network error"

[[mocks.file_read]]
node = "load_table"
error = "mock read error"

[[mocks.file_write]]
node = "write_table"
error = "mock write error"
```

## recorded effects

mock された副作用は `recorded_effects[]` として report に記録されます。
`kind` は `http` / `file_read` / `file_write` のいずれかです。

主な field:

| kind | field |
|---|---|
| `http` | `node`, `method`, `url`, `status`, `request_body`, `response_body`, `error` |
| `file_read` | `node`, `path`, `bytes`, `contents`, `error` |
| `file_write` | `node`, `path`, `bytes`, `contents`, `error` |

外部 IO のテストでは、trace 文字列より recorded effects の構造化 assertion を優先します。

```toml
[[tests.expect.http_requests]]
node = "request"
method = "POST"
url = "https://example.invalid/webhook"
status = 200
request_body = "{\"message\":\"hello\"}"
response_body = "{\"ok\":true}"

[[tests.expect.file_reads]]
node = "load_table"
path = "input.tsv"
contents = "name\tscore\namiya\t100\n"

[[tests.expect.file_writes]]
node = "write_table"
path = "output.tsv"
contents = "name\tscore\namiya\t100\n"
```

失敗を期待する場合は `error` を指定します。

```toml
[[tests.expect.file_writes]]
node = "write_table"
path = "output.tsv"
error = "mock write error"
```

## 使い分け

- Flowgraph 言語機能、ノードの入出力、mock 可能な副作用は Flowgraph fixture で見る。
- GUI、Control API、WebSocket、ブラウザー上の操作は Playwright E2E で見る。
- 実サービス連携は最小限にし、通常の回帰確認では mock fixture を優先する。

現状 CI 化は optional。まずはローカルで `--flowgraph-test-root flowgraph.example` を回し、fixture と実装の差分を小さく保つ運用を標準とします。
