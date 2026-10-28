#!/usr/bin/env pwsh
# Creates dev/config.toml (gitignored) for the local web deployment started by the Procfile:
# backend on :3001, CORS open for the dev web server. Safe to re-run, keeps an existing config.
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')
$config = 'dev/config.toml'
if (-not (Test-Path $config)) {
    $lines = cargo run -q -p macworp_backend -- config
    if ($LASTEXITCODE -ne 0) { throw 'cargo run failed' }
    $lines = $lines `
        -replace '^port = .*', 'port = 3001' `
        -replace '^cors_allowed_origins = .*', 'cors_allowed_origins = ["*"]' `
        -replace '^projects_path = .*', 'projects_path = "dev_projects"'
    # UTF-8 without BOM and LF endings; Windows PowerShell 5.1 `>` would write UTF-16.
    [System.IO.File]::WriteAllText(
        (Join-Path (Get-Location) $config),
        (($lines -join "`n") + "`n"),
        (New-Object System.Text.UTF8Encoding $false))
    Write-Host "wrote $config"
}
@'
Next: start the three Procfile processes (`ultraman start`, or one terminal each on Windows):
  docker compose up db
  cargo run -p macworp_backend -- db-migrate dev/config.toml; cargo run -p macworp_backend -- serve dev/config.toml
  cd packages/web; $env:MACWORP_BASE_URL = 'http://127.0.0.1:3001'; dx serve --web --port 5001
Then create users (prompts for a password), e.g. in another terminal:
  cargo run -p macworp_backend -- user create alice Alice A alice@example.org
  cargo run -p macworp_backend -- user create bob Bob B bob@example.org
Web UI: http://127.0.0.1:5001 (log in in two browser profiles to try sharing).
'@ | Write-Host
