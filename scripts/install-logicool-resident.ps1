#Requires -RunAsAdministrator
[CmdletBinding()]
param(
    [Parameter(Mandatory, ParameterSetName='Install')]
    [string]$Executable,
    [Parameter(Mandatory, ParameterSetName='Restore')]
    [switch]$Restore
)
$ErrorActionPreference = 'Stop'
$user = [Security.Principal.WindowsIdentity]::GetCurrent()
$folder = Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'Device HUB Logicool'
$binary = Join-Path $folder 'device-hub.exe'
$name = 'Device HUB Logicool ' + $user.User.Value
$run = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$backup = 'HKCU:\Software\DeviceHub\LogicoolStartup'
$existing = Get-ScheduledTask -TaskName $name -ErrorAction SilentlyContinue
if ($existing -and ($existing.Actions.Count -ne 1 -or $existing.Actions[0].Execute -ne $binary)) {
    throw 'An unrelated task already uses this name.'
}
if ($Restore) {
    if ($existing) {
        Stop-ScheduledTask -TaskName $name
        Unregister-ScheduledTask -TaskName $name -Confirm:$false
    }
    if (Test-Path -LiteralPath $backup) {
        & (Join-Path $PSScriptRoot 'logicool-startup.ps1') -Restore
    }
    return
}
$source = Get-Item -LiteralPath $Executable
if ($source.Name -ne 'device-hub.exe') {throw 'Select the built device-hub.exe.'}
$old = (Get-Item -LiteralPath $run).GetValue('LGHUB')
if ($old -and $old -notmatch '(?i)\\LGHUB\\.*lghub[^"]*\.exe') {throw 'Unrecognized G HUB startup entry.'}
if (!(Test-Path -LiteralPath $folder)) {
    New-Item -ItemType Directory -Path $folder | Out-Null
}
if ((Get-Item -LiteralPath $folder).Attributes.HasFlag([IO.FileAttributes]::ReparsePoint)) {
    throw 'Unexpected application folder link.'
}
if ((Test-Path -LiteralPath $binary) -and (Get-Item -LiteralPath $binary).Attributes.HasFlag([IO.FileAttributes]::ReparsePoint)) {
    throw 'Unexpected executable link.'
}
if ($existing) {Stop-ScheduledTask -TaskName $name}
Copy-Item -LiteralPath $source.FullName -Destination $binary -Force
if ((Get-FileHash -LiteralPath $binary).Hash -ne (Get-FileHash -LiteralPath $source.FullName).Hash) {
    throw 'Installed executable does not match the build.'
}
$action = New-ScheduledTaskAction -Execute $binary -Argument '--resident' -WorkingDirectory $folder
$trigger = New-ScheduledTaskTrigger -AtLogOn -User $user.Name
$principal = New-ScheduledTaskPrincipal -UserId $user.Name -LogonType Interactive -RunLevel Highest
$settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -ExecutionTimeLimit ([TimeSpan]::Zero) -MultipleInstances IgnoreNew -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 1)
Register-ScheduledTask -TaskName $name -Action $action -Trigger $trigger -Principal $principal -Settings $settings -Force | Out-Null
Start-ScheduledTask -TaskName $name
$until = (Get-Date).AddSeconds(45)
do {
    Start-Sleep -Milliseconds 500
    $app = Get-Process -Name device-hub -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $binary -and $_.MainWindowHandle -ne 0 }
} until ($app -or (Get-Date) -gt $until)
if (!$app) {
    Unregister-ScheduledTask -TaskName $name -Confirm:$false
    throw 'Device HUB did not open a window. G HUB startup was preserved.'
}
New-Item -Path $backup -Force | Out-Null
if ($old -and !(Get-Item -LiteralPath $backup).GetValue('PreviousLGHUB')) {
    New-ItemProperty -LiteralPath $backup -Name PreviousLGHUB -Value $old -PropertyType String | Out-Null
}
if ($old) {Remove-ItemProperty -LiteralPath $run -Name LGHUB}
# Release the vendor HID client after the replacement has opened successfully.
$vendor = Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'LGHUB'
foreach ($item in @('system_tray\lghub_system_tray.exe','lghub.exe','lghub_agent.exe')) {
    $expected = Join-Path $vendor $item
    $processName = [IO.Path]::GetFileNameWithoutExtension($item)
    Get-Process -Name $processName -ErrorAction SilentlyContinue |
        Where-Object { $_.Path -eq $expected } |
        Stop-Process -ErrorAction SilentlyContinue
}
Write-Output "Running: $binary"
