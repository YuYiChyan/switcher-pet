param([switch]$Settings)
$ErrorActionPreference='Stop'
$exe=Join-Path $PSScriptRoot 'switcher-pet.exe'
if(-not(Test-Path -LiteralPath $exe)){throw 'Keep this launcher beside switcher-pet.exe.'}
$exe=[IO.Path]::GetFullPath($exe)
$running=Get-CimInstance Win32_Process -Filter "Name='switcher-pet.exe'"|Where-Object {$_.ExecutablePath -eq $exe -and $_.CommandLine -notmatch '--validation'}|Select-Object -First 1
if(-not $running){
    $arguments=if($Settings){'--preview --settings'}else{'--preview'}
    Start-Process -FilePath $exe -ArgumentList $arguments -WorkingDirectory $PSScriptRoot -WindowStyle Hidden
    return
}
# Reopening a shortcut restores only this package's existing window, instead of
# launching a second account manager that will fail the profile lock.
Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class SwitcherPetLauncher {
 public delegate bool Callback(IntPtr hwnd,IntPtr value);
 [DllImport("user32.dll")] public static extern bool EnumWindows(Callback cb,IntPtr value);
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd,out uint pid);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hwnd,StringBuilder text,int count);
 [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hwnd,int command);
 [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
}
'@
$script:owner=[uint32]$running.ProcessId
$script:target=[IntPtr]::Zero
$script:fallback=[IntPtr]::Zero
$script:settingsMode=[bool]$Settings
$callback=[SwitcherPetLauncher+Callback]{param($handle,$unused)
    $ownerId=[uint32]0;[void][SwitcherPetLauncher]::GetWindowThreadProcessId($handle,[ref]$ownerId)
    if($ownerId -eq $script:owner){
        $title=[Text.StringBuilder]::new(512);[void][SwitcherPetLauncher]::GetWindowText($handle,$title,512)
        if($title.ToString().StartsWith('Switcher Pet')){
            $isPet=$title.ToString().Contains('桌宠')
            $script:fallback=$handle
            if(($script:settingsMode -and -not $isPet) -or (-not $script:settingsMode -and $isPet)){$script:target=$handle}
        }
    }
    return $true
}
[void][SwitcherPetLauncher]::EnumWindows($callback,[IntPtr]::Zero)
if($script:target -eq [IntPtr]::Zero){$script:target=$script:fallback}
if($script:target -ne [IntPtr]::Zero){[void][SwitcherPetLauncher]::ShowWindow($script:target,9);[void][SwitcherPetLauncher]::SetForegroundWindow($script:target)}
