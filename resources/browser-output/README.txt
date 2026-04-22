OBS ブラウザソース用のサンプル配置です。conf で

  [browser_source]
  document_root = "resources/browser-output"

のようにすると、http://127.0.0.1:57000/browser-output/<フォルダ>/index.html で開けます。
従来の GET /output は document_root の index.html を返します。POST /output API は変更ありません。
