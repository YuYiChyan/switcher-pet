[CmdletBinding()]
param(
    [string]$Source = (Join-Path $HOME '.codex-switcher/accounts.json'),
    [string]$Destination = (Join-Path $HOME '.switcher-pet/preview/profile/.codex-switcher/accounts.json'),
    [switch]$Apply
)
$ErrorActionPreference = 'Stop'
$Source = [IO.Path]::GetFullPath($Source)
$Destination = [IO.Path]::GetFullPath($Destination)
$owned = [IO.Path]::GetFullPath((Join-Path $HOME '.switcher-pet')) + [IO.Path]::DirectorySeparatorChar
if (-not $Destination.StartsWith($owned,[StringComparison]::OrdinalIgnoreCase) -or $Source -eq $Destination) { throw 'Destination must be a separate fork profile.' }
if (Get-CimInstance Win32_Process -Filter "Name='switcher-pet.exe'") { throw 'Close the independent fork before repairing its configuration.' }
foreach ($path in @($Source,$Destination)) {
    $item=Get-Item -LiteralPath $path
    while ($item) {
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) {throw 'Refusing redirected configuration paths.'}
        $item=if($item -is [IO.FileInfo]){$item.Directory}else{$item.Parent}
    }
}
$sourceHash=(Get-FileHash -LiteralPath $Source).Hash
$before=[IO.File]::ReadAllBytes($Destination)
$original=Get-Content -Raw -LiteralPath $Source|ConvertFrom-Json -AsHashtable
$fork=[Text.Encoding]::UTF8.GetString($before)|ConvertFrom-Json -AsHashtable
$count=0
foreach ($a in $fork.accounts.Values) {
    # Exact static-key match only; names/emails alone never decide credential routing.
    $key=$a.auth_json.tokens.access_token
    if (-not $key -or $key.StartsWith('eyJ') -or $a.kind -eq 'antigravity_oauth' -or $a.relay_base_url) {continue}
    $matches=@($original.accounts.Values|Where-Object {$_.kind -eq 'relay' -and $_.relay_base_url -and $_.auth_json.tokens.access_token -ceq $key})
    if ($matches.Count -ne 1) {continue}
    $match=$matches[0]
    foreach ($field in @('kind','notes','relay_base_url','relay_homepage','relay_protocol','relay_usage_preset','relay_model_map','relay_model_fallback','relay_model_catalog','relay_category')) {
        if ($match.ContainsKey($field)) {$a[$field]=$match[$field]}
    }
    # Keep the invalid flag until the real read-only probe verifies the credential.
    # Never replace auth, IDs, OAuth refresh tokens or global proxy settings.
    $count++
}
if (-not $Apply) { [pscustomobject]@{RepairableApiSources=$count;Applied=$false}; return }
if ($count -eq 0) { [pscustomobject]@{RepairableApiSources=0;Applied=$false}; return }
$backup=Join-Path (Split-Path $Destination -Parent) ('accounts.before-api-repair-'+[DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ')+'.dpapi')
$protected=[Security.Cryptography.ProtectedData]::Protect($before,$null,[Security.Cryptography.DataProtectionScope]::CurrentUser)
[IO.File]::WriteAllBytes($backup,$protected)
if ([Convert]::ToBase64String([IO.File]::ReadAllBytes($Destination)) -cne [Convert]::ToBase64String($before)) {throw 'Configuration changed during repair.'}
$temp=$Destination+'.repair.tmp'
[IO.File]::WriteAllText($temp,($fork|ConvertTo-Json -Depth 100),[Text.UTF8Encoding]::new($false))
Move-Item -LiteralPath $temp -Destination $Destination -Force
if ((Get-FileHash -LiteralPath $Source).Hash -ne $sourceHash) {throw 'Original configuration changed concurrently.'}
[pscustomobject]@{RepairedApiSources=$count;Applied=$true;Backup=$backup;OriginalUnchanged=$true}
