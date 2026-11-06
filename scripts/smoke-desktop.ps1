[CmdletBinding()]
param([string]$ExePath = '', [switch]$NoPreviewArgument, [switch]$RoutingTests)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
if (Test-Path -LiteralPath (Join-Path $repo '.tooling/mingw64/x86_64-w64-mingw32/lib/default-manifest.o.switcher-pet-disabled')) { throw 'Build is still running; wait for completion before launching its executable.' }
if (-not $ExePath) { $ExePath = Join-Path $repo 'src-tauri/target/release/switcher-pet.exe' }
$exe = (Resolve-Path -LiteralPath $ExePath).Path
if ([IO.Path]::GetFileName($exe) -cne 'switcher-pet.exe') { throw 'Only the fork executable can be tested.' }
$python = Join-Path $HOME '.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe'
$root = Join-Path $HOME '.switcher-pet'
if ($NoPreviewArgument -and -not (Test-Path -LiteralPath (Join-Path $root 'import-pending.json'))) {
    throw 'Default-start test requires a paused import, otherwise it might open a real profile.'
}
$profileDirectory = if ($RoutingTests) { 'validation' } else { 'preview' }
$previewStore = Join-Path $root "$profileDirectory/profile/.codex-switcher/accounts.json"
if (Test-Path -LiteralPath $previewStore) {
    $data = Get-Content -LiteralPath $previewStore -Raw | ConvertFrom-Json -AsHashtable
    if ($data.accounts.Count -ne 0 -or $data.settings.proxy_enabled -or $data.settings.background_refresh -or $data.settings.remote_mode -ne 'off') {
        throw 'Smoke test requires an empty, inactive preview profile.'
    }
}
$protected = @(
    (Join-Path $HOME 'AppData/Local/Codex Switcher/codex-switcher.exe'),
    (Join-Path $root 'profile/.codex-switcher/accounts.json'),
    (Join-Path $root 'import-pending.json')
)
$hashes = @{}
foreach ($path in $protected) { if (Test-Path -LiteralPath $path) { $hashes[$path] = (Get-FileHash -LiteralPath $path).Hash } }
$listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
$listener.Start(); $port = $listener.LocalEndpoint.Port; $listener.Stop()
$tag = [Guid]::NewGuid().ToString('N')
$output = Join-Path $repo ('.tooling/smoke-' + $tag)
$saved = @{}
foreach ($name in @('WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS','WEBVIEW2_USER_DATA_FOLDER','PYTHONPATH')) {
    $saved[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
}
$process = $null
$previewSnapshots = @{}
if ($RoutingTests) {
    foreach ($name in @('accounts.json','session_routes.json','probe-status.json','../pet-sounds/press.audio','../pet-sounds/release.audio','../.codex/models_cache.json','../.codex/models_cache.codex_work_desktop.json')) {
        $path = Join-Path $root ($profileDirectory + '/profile/.codex-switcher/' + $name)
        if (Test-Path -LiteralPath $path) {
            if ($name -eq 'session_routes.json') {
                $routes = Get-Content -Raw -LiteralPath $path | ConvertFrom-Json -AsHashtable
                if ($routes.routes.Count -ne 0) { throw 'Routing tests require an empty preview route table.' }
            }
            $previewSnapshots[$path] = [IO.File]::ReadAllBytes($path)
        } else { $previewSnapshots[$path] = $null }
    }
}
try {
    $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$port --remote-debugging-address=127.0.0.1"
    $env:WEBVIEW2_USER_DATA_FOLDER = Join-Path $output 'webview'
    $env:PYTHONPATH = Join-Path $repo '.tooling/python-packages'
    $params = @{ FilePath=$exe; WorkingDirectory=(Split-Path -Parent $exe); PassThru=$true; WindowStyle='Hidden' }
    if ($RoutingTests) { $params.ArgumentList = '--validation' }
    elseif (-not $NoPreviewArgument) { $params.ArgumentList = '--preview' }
    $process = Start-Process @params
    $deadline = [DateTime]::UtcNow.AddSeconds(60)
    $ready = $false
    while ([DateTime]::UtcNow -lt $deadline) {
        $process.Refresh()
        if ($process.HasExited) { throw "Preview exited before WebView was ready: $($process.ExitCode)" }
        try {
            $version = Invoke-RestMethod "http://127.0.0.1:$port/json/version" -TimeoutSec 1
            $pages = Invoke-RestMethod "http://127.0.0.1:$port/json/list" -TimeoutSec 1
            if ($version.webSocketDebuggerUrl -and @($pages | Where-Object { $_.type -eq 'page' -and $_.url -match 'tauri.localhost' }).Count -gt 0) { $ready = $true; break }
            Start-Sleep -Milliseconds 400
        } catch { Start-Sleep -Milliseconds 400 }
    }
    if (-not $ready) { throw 'Preview WebView did not become ready within 60 seconds.' }
    $script = if ($RoutingTests) { 'scripts/test-routing-desktop.py' } else { 'scripts/smoke-desktop.py' }
    & $python (Join-Path $repo $script) "http://127.0.0.1:$port" $output
    if ($LASTEXITCODE -ne 0) { throw 'Native desktop smoke test failed.' }
    Write-Output "Smoke evidence: $output"
} finally {
    # Stop only the exact process we started, never the original or a name wildcard.
    if ($null -ne $process) {
        $process.Refresh()
        if (-not $process.HasExited) {
            $current = Get-Process -Id $process.Id
            if ($current.Path -ne $exe -or $current.StartTime -ne $process.StartTime) { throw 'Process identity changed; refusing cleanup.' }
            Stop-Process -Id $process.Id
            $process.WaitForExit(10000) | Out-Null
        }
    }
    foreach ($name in $saved.Keys) { [Environment]::SetEnvironmentVariable($name, $saved[$name], 'Process') }
    foreach ($path in $previewSnapshots.Keys) {
        if ($null -eq $previewSnapshots[$path]) {
            if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Force }
        } else { [IO.File]::WriteAllBytes($path, $previewSnapshots[$path]) }
    }
    foreach ($path in $hashes.Keys) {
        if ((Get-FileHash -LiteralPath $path).Hash -ne $hashes[$path]) { throw "Protected file changed: $path" }
    }
}
