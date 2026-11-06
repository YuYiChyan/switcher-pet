$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$fixture = Join-Path $tempRoot ('switcher-pet-import-test-' + [Guid]::NewGuid())
[void][IO.Directory]::CreateDirectory($fixture)
function Assert($ok, [string]$message) { if (-not $ok) { throw $message } }
try {
    $source = Join-Path $fixture 'source'
    $dest = Join-Path $fixture 'dest'
    [void][IO.Directory]::CreateDirectory((Join-Path $source '.codex-switcher'))
    [void][IO.Directory]::CreateDirectory($dest)
    $original = '{"version":1,"current":"fake","accounts":{"fake":{"id":"fake","refresh_token":"synthetic-only","keepalive":{"inactive_refresh_enabled":true},"window_priming":{"five_hour_enabled":true,"weekly_enabled":true}}},"settings":{"proxy_enabled":true,"proxy_port":18080,"background_refresh":true,"remote_mode":"server"}}'
    $sourceFile = Join-Path $source '.codex-switcher/accounts.json'
    [IO.File]::WriteAllText($sourceFile, $original)
    [IO.File]::WriteAllText((Join-Path $source '.codex-switcher/session_routes.json'), '{"version":1,"routes":{"test":{"enabled":true}}}')
    & "$PSScriptRoot/copy-config-isolated.ps1" -SourceHome $source -DestinationHome $dest | Out-Null
    $root = Join-Path $dest '.switcher-pet'
    Assert ([IO.File]::ReadAllText($sourceFile) -ceq $original) 'Source modified'
    Assert (Test-Path -LiteralPath (Join-Path $root 'import-pending.json')) 'Missing runtime pause marker'
    $copy = Get-Content -Raw (Join-Path $root 'profile/.codex-switcher/accounts.json') | ConvertFrom-Json
    Assert ($copy.accounts.fake.refresh_token -ceq 'synthetic-only') 'Credential not preserved'
    Assert (-not $copy.settings.proxy_enabled -and -not $copy.settings.background_refresh -and $copy.settings.proxy_port -eq 18080) 'Unsafe settings'
    Assert (-not $copy.accounts.fake.keepalive.inactive_refresh_enabled -and -not $copy.accounts.fake.window_priming.five_hour_enabled) 'Background account activity enabled'
    $routes = Get-Content -Raw (Join-Path $root 'profile/.codex-switcher/session_routes.json') | ConvertFrom-Json
    Assert (-not $routes.routes.test.enabled) 'Route still enabled'
    $backup = Get-ChildItem -LiteralPath (Join-Path $root 'imports') -Filter 'accounts.json.dpapi' -Recurse
    $plain = [Security.Cryptography.ProtectedData]::Unprotect([IO.File]::ReadAllBytes($backup.FullName), $null, [Security.Cryptography.DataProtectionScope]::CurrentUser)
    Assert ([Text.Encoding]::UTF8.GetString($plain) -ceq $original) 'Backup is not exact/recoverable'
    $acl = Get-Acl -LiteralPath $root
    Assert ($acl.AreAccessRulesProtected -and @($acl.Access).Count -eq 2) 'Private ACL missing'
    $refused = $false
    try { & "$PSScriptRoot/copy-config-isolated.ps1" -SourceHome $source -DestinationHome $dest | Out-Null } catch { $refused = $true }
    Assert $refused 'Existing destination was overwritten'
    Write-Output 'PASS: synthetic import, source preservation, pause marker, disabled activity/routes, exact DPAPI recovery, private ACL, overwrite refusal.'
} finally {
    $resolved = [IO.Path]::GetFullPath($fixture)
    if (-not $resolved.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -or -not ([IO.Path]::GetFileName($resolved)).StartsWith('switcher-pet-import-test-')) {
        throw 'Unsafe test cleanup path'
    }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
