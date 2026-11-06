$ErrorActionPreference='Stop'
$owned=[IO.Path]::GetFullPath((Join-Path $HOME '.switcher-pet'))
$fixture=Join-Path $owned ('repair-fixture-'+[Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $fixture|Out-Null
try {
    $source=Join-Path $fixture 'original.json';$target=Join-Path $fixture 'fork.json'
    $key='sk-synthetic-not-a-real-credential'
    $original=@{accounts=@{old=@{kind='relay';name='Original';relay_base_url='https://provider.invalid/v1';relay_protocol='responses';auth_json=@{tokens=@{access_token=$key}}}}}
    $local=@{accounts=@{new=@{id='new';name='Renamed';kind='legacy';relay_base_url=$null;is_token_invalid=$true;auth_json=@{tokens=@{access_token=$key}}};different=@{id='different';name='Original';kind='legacy';auth_json=@{tokens=@{access_token='sk-different'}}}};settings=@{proxy_enabled=$false}}
    $original|ConvertTo-Json -Depth 20|Set-Content $source -Encoding utf8
    $local|ConvertTo-Json -Depth 20|Set-Content $target -Encoding utf8
    $hash=(Get-FileHash $source).Hash
    & (Join-Path $PSScriptRoot 'repair-imported-api-metadata.ps1') -Source $source -Destination $target -Apply|Out-Null
    $after=Get-Content $target -Raw|ConvertFrom-Json -AsHashtable
    if($after.accounts.new.kind -ne 'relay' -or $after.accounts.new.id -ne 'new' -or $after.accounts.new.name -ne 'Renamed'){throw 'Identity or metadata repair failed'}
    if(-not $after.accounts.new.is_token_invalid){throw 'Repair incorrectly claimed API health'}
    if($after.accounts.different.kind -ne 'legacy'){throw 'Name-only match leaked provider routing'}
    if((Get-FileHash $source).Hash -ne $hash){throw 'Original changed'}
    if(-not(Get-ChildItem $fixture -Filter '*.dpapi')){throw 'Missing encrypted backup'}
    'PASS: exact key metadata recovery, preserved identity/auth/invalid state, name mismatch ignored, original unchanged, encrypted backup.'
} finally {
    $resolved=[IO.Path]::GetFullPath($fixture)
    if(-not $resolved.StartsWith($owned+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)){throw 'Unsafe fixture cleanup'}
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
