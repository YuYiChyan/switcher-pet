# Copies configuration only. Never launches an app or contacts an account provider.
[CmdletBinding()]
param(
    [string]$SourceHome = $HOME,
    [string]$DestinationHome = $HOME
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Assert-PlainPath([string]$Path) {
    $item = Get-Item -LiteralPath $Path -Force
    while ($null -ne $item) {
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) {
            throw 'Refusing a redirected configuration path.'
        }
        if ($item -is [IO.FileInfo]) { $item = $item.Directory } else { $item = $item.Parent }
    }
}
function Read-StableBytes([string]$Path) {
    Assert-PlainPath $Path
    for ($attempt = 0; $attempt -lt 3; $attempt++) {
        $bytes = [IO.File]::ReadAllBytes($Path)
        $again = [IO.File]::ReadAllBytes($Path)
        if ([Convert]::ToBase64String($bytes) -ceq [Convert]::ToBase64String($again)) { return ,$bytes }
    }
    throw 'Source configuration is changing. No import was created; retry later.'
}
function Write-Json([string]$Path, $Value) {
    [IO.File]::WriteAllText($Path, (ConvertTo-Json -InputObject $Value -Depth 100), [Text.UTF8Encoding]::new($false))
}

$SourceHome = [IO.Path]::GetFullPath($SourceHome)
$DestinationHome = [IO.Path]::GetFullPath($DestinationHome)
Assert-PlainPath $SourceHome
Assert-PlainPath $DestinationHome
$root = Join-Path $DestinationHome '.switcher-pet'
if (Test-Path -LiteralPath $root) { throw 'Destination already exists; refusing to merge or overwrite a profile.' }
$source = Join-Path $SourceHome '.codex-switcher'
$raw = Read-StableBytes (Join-Path $source 'accounts.json')
$accounts = [Text.Encoding]::UTF8.GetString($raw) | ConvertFrom-Json -AsHashtable
if ($accounts.accounts -isnot [System.Collections.IDictionary] -or $accounts.settings -isnot [System.Collections.IDictionary]) {
    throw 'Unsupported account-store shape.'
}
$snapshots = @{ 'accounts.json' = $raw }
foreach ($name in @('session_routes.json', 'skills.json')) {
    $path = Join-Path $source $name
    if (Test-Path -LiteralPath $path) { $snapshots[$name] = Read-StableBytes $path }
}
$codexConfig = Join-Path $SourceHome '.codex/config.toml'
if (Test-Path -LiteralPath $codexConfig) { $snapshots['codex-config.toml'] = Read-StableBytes $codexConfig }
$routes = @{ version = 1; routes = @{} }
if ($snapshots.ContainsKey('session_routes.json')) {
    $routes = [Text.Encoding]::UTF8.GetString($snapshots['session_routes.json']) | ConvertFrom-Json -AsHashtable
    if ($routes.routes -isnot [System.Collections.IDictionary]) { throw 'Unsupported route-store shape.' }
}

# Private ACL is installed before writing any credential. Backups additionally use
# Windows DPAPI CurrentUser, preserving the exact original bytes for recovery.
[void][IO.Directory]::CreateDirectory($root)
$acl = [Security.AccessControl.DirectorySecurity]::new()
$acl.SetAccessRuleProtection($true, $false)
$user = [Security.Principal.WindowsIdentity]::GetCurrent().User
$system = [Security.Principal.SecurityIdentifier]::new('S-1-5-18')
foreach ($sid in @($user, $system)) {
    $rule = [Security.AccessControl.FileSystemAccessRule]::new($sid, 'FullControl', 'ContainerInherit,ObjectInherit', 'None', 'Allow')
    $acl.AddAccessRule($rule)
}
Set-Acl -LiteralPath $root -AclObject $acl
Write-Json (Join-Path $root 'import-pending.json') @{
    state = 'paused'; reason = 'Resolve OAuth refresh ownership before activation. Do not simply delete this marker.'
    created_utc = [DateTime]::UtcNow.ToString('o')
}
$archive = Join-Path $root ('imports/' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ'))
[void][IO.Directory]::CreateDirectory($archive)
foreach ($name in $snapshots.Keys) {
    $encrypted = [Security.Cryptography.ProtectedData]::Protect($snapshots[$name], $null, [Security.Cryptography.DataProtectionScope]::CurrentUser)
    [IO.File]::WriteAllBytes((Join-Path $archive ($name + '.dpapi')), $encrypted)
}

$settings = $accounts.settings
foreach ($key in @('auto_reload_ide','use_pkill_restart','background_refresh','proxy_enabled','proxy_allow_lan',
    'quota_refresh_enabled','solo_auto_sync_current','relay_auto_switch_out','relay_auto_switch_in',
    'client_direct_upstream','client_owns_current','skills_auto_pull','allow_auto_switch_to_free','inject_switch_message')) {
    $settings[$key] = $false
}
$settings['proxy_port'] = 18080
$settings['remote_server_port'] = 28181
$settings['remote_server_bind'] = '127.0.0.1'
$settings['remote_mode'] = 'off'
foreach ($key in @('remote_server_url','remote_server_url_fallback','remote_shared_secret')) { $settings[$key] = '' }
foreach ($account in $accounts.accounts.Values) {
    $account['is_session_anchor'] = $false
    if (-not $account.ContainsKey('keepalive')) { $account['keepalive'] = @{} }
    $account.keepalive['inactive_refresh_enabled'] = $false
    if (-not $account.ContainsKey('window_priming')) { $account['window_priming'] = @{} }
    $account.window_priming['five_hour_enabled'] = $false
    $account.window_priming['weekly_enabled'] = $false
}
foreach ($route in $routes.routes.Values) { $route['enabled'] = $false }
$profile = Join-Path $root 'profile/.codex-switcher'
[void][IO.Directory]::CreateDirectory($profile)
Write-Json (Join-Path $profile 'accounts.json') $accounts
Write-Json (Join-Path $profile 'session_routes.json') $routes
$manifest = @{
    state = 'paused'; account_count = $accounts.accounts.Count; route_count = $routes.routes.Count
    snapshot_files = @($snapshots.Keys | Sort-Object); backup_encryption = 'Windows DPAPI CurrentUser'
    excluded = @('Codex auth.json', 'sessions', 'history/logs', 'skill files and links')
    notes = 'Skills metadata and Codex config are archived only, not activated. Source files are read-only.'
}
Write-Json (Join-Path $archive 'manifest.json') $manifest
# Output only non-sensitive counts and destination; never print credentials or IDs.
[PSCustomObject]@{ State = 'paused'; Accounts = $accounts.accounts.Count; Routes = $routes.routes.Count; Destination = $root }
