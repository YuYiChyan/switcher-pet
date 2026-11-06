# Local portable Windows toolchain. Does not install, launch or replace either app.
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$targetExe=Join-Path $repo 'src-tauri/target/release/switcher-pet.exe'
if (Get-Process switcher-pet -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $targetExe }) { throw 'The build output is running; close only your own validation run before building. Published preview packages do not block builds.' }
$names = @('PATH','CARGO_HOME','CARGO_BUILD_JOBS','CC','CXX','AR','CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER','CARGO_ENCODED_RUSTFLAGS')
$saved = @{}
$manifestObject = Join-Path $repo '.tooling/mingw64/x86_64-w64-mingw32/lib/default-manifest.o'
$manifestBackup = $manifestObject + '.switcher-pet-disabled'
$manifestMoved = $false
foreach ($name in $names) { $saved[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
Push-Location $repo
try {
    foreach ($path in @('.tooling/mingw64/bin/gcc.exe','.tooling/mingw64/bin/strip.exe','.tooling/rust/bin/cargo.exe','.tooling/bin/npm.cmd','node_modules/@tauri-apps/cli/tauri.js')) {
        if (-not (Test-Path -LiteralPath $path)) { throw "Missing local build prerequisite: $path" }
    }
    $env:PATH = "$repo\.tooling\mingw64\bin;$repo\.tooling\bin;$repo\.tooling\rust\bin;" + $env:PATH
    $env:CARGO_HOME = "$repo\.tooling\cargo-home"
    $env:CARGO_BUILD_JOBS = '4'
    # Published panic/source locations must not disclose the build user's home.
    $flags = if ($env:CARGO_ENCODED_RUSTFLAGS) { $env:CARGO_ENCODED_RUSTFLAGS } else { ($env:RUSTFLAGS -split '\s+' | Where-Object { $_ }) -join [char]31 }
    $remaps = @("--remap-path-prefix=$env:USERPROFILE=/build-user", "--remap-path-prefix=$($env:USERPROFILE.Replace('\','/'))=/build-user", "--remap-path-prefix=$repo=/src")
    $env:CARGO_ENCODED_RUSTFLAGS = (@($flags | Where-Object { $_ }) + $remaps) -join [char]31
    $env:CC = 'gcc'
    $env:CXX = 'g++'
    $env:AR = 'ar'
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = "$repo\.tooling\mingw64\bin\gcc.exe"
    # WinLibs GCC16 injects another manifest; GNU ld cannot merge it with Tauri's.
    # Upstream workaround: https://github.com/brechtsanders/winlibs_mingw/issues/299
    # Only touch our private portable toolchain, restoring it even on build failure.
    if (Test-Path -LiteralPath $manifestBackup) { throw 'A previous build left a manifest backup; inspect before rebuilding.' }
    if (Test-Path -LiteralPath $manifestObject) {
        Rename-Item -LiteralPath $manifestObject -NewName ([IO.Path]::GetFileName($manifestBackup))
        $manifestMoved = $true
    }
    & node node_modules/@tauri-apps/cli/tauri.js build --no-bundle -- --locked 2>&1 | Tee-Object .tooling/desktop-build.log
    if ($LASTEXITCODE -ne 0) { throw "Desktop build failed (exit $LASTEXITCODE). See .tooling/desktop-build.log." }
    # MinGW import-library symbol names embed absolute build paths independently
    # of Rust source remapping. They are not needed by the shipped executable.
    & .tooling/mingw64/bin/strip.exe --strip-all $targetExe
    if ($LASTEXITCODE -ne 0) { throw 'Could not strip private linker symbols.' }
    & node scripts/verify-windows-artifact.mjs src-tauri/target/release/switcher-pet.exe
    if ($LASTEXITCODE -ne 0) { throw 'Windows artifact verification failed.' }
} finally {
    if ($manifestMoved) { Rename-Item -LiteralPath $manifestBackup -NewName ([IO.Path]::GetFileName($manifestObject)) }
    foreach ($name in $names) { [Environment]::SetEnvironmentVariable($name, $saved[$name], 'Process') }
    Pop-Location
}
