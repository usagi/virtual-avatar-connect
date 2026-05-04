$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Push-Location $repoRoot
try {
    $env:BLESS_NODE_CATALOG = "1"
    cargo test --lib node_catalog_md_up_to_date
}
finally {
    Remove-Item Env:BLESS_NODE_CATALOG -ErrorAction SilentlyContinue
    Pop-Location
}