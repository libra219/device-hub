param(
    [Parameter(Mandatory, ParameterSetName='Enable')]
    [string]$Executable,
    [Parameter(Mandatory, ParameterSetName='Restore')]
    [switch]$Restore
)
$ErrorActionPreference = 'Stop'
$run = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$state = 'HKCU:\Software\DeviceHub\LogicoolStartup'
$entry = 'Device HUB (Logicool)'
$key = Get-Item -LiteralPath $run
if ($Restore) {
    $saved = Get-Item -LiteralPath $state
    $previous = $saved.GetValue('PreviousLGHUB')
    if ($previous -and !$key.GetValue('LGHUB')) {
        New-ItemProperty -LiteralPath $run -Name LGHUB -Value $previous -PropertyType String | Out-Null
    }
    if ($key.GetValue($entry) -eq $saved.GetValue('Command')) {
        Remove-ItemProperty -LiteralPath $run -Name $entry
    }
    Remove-Item -LiteralPath $state
    return
}
$file = Get-Item -LiteralPath $Executable
if ($file.Name -ne 'device-hub.exe') { throw 'Select the built device-hub.exe.' }
$command = '"{0}"' -f $file.FullName
$old = $key.GetValue('LGHUB')
if ($old -and $old -notmatch '(?i)\\LGHUB\\.*lghub[^"]*\.exe') {
    throw 'Unrecognized LGHUB startup command.'
}
if ($key.GetValue($entry) -and $key.GetValue($entry) -ne $command) {
    throw 'Another Device HUB startup registration already exists.'
}
New-Item -Path $state -Force | Out-Null
if ($old -and !(Get-Item -LiteralPath $state).GetValue('PreviousLGHUB')) {
    New-ItemProperty -LiteralPath $state -Name PreviousLGHUB -Value $old -PropertyType String | Out-Null
}
Set-ItemProperty -LiteralPath $state -Name Command -Value $command
New-ItemProperty -LiteralPath $run -Name $entry -Value $command -PropertyType String -Force | Out-Null
if ((Get-Item -LiteralPath $run).GetValue($entry) -ne $command) { throw 'Startup registration failed.' }
if ($old) { Remove-ItemProperty -LiteralPath $run -Name LGHUB }
