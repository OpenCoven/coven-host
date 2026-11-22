# Run only on a disposable Windows Actions host. The account and task belong to this run.
param([Parameter(Mandatory=$true)][string]$Installer)
$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_OS -ne 'Windows') { throw 'A disposable Windows Actions runner is required' }
$name = 'CovenQA' + [Guid]::NewGuid().ToString('N').Substring(0,8)
$fixture = Join-Path $env:SystemDrive $name
$password = [Guid]::NewGuid().ToString('N') + 'aA2!'
$identity = "$env:COMPUTERNAME\$name"
$userCreated = $false
$taskCreated = $false
try {
    New-LocalUser -Name $name -Password (ConvertTo-SecureString -String $password -AsPlainText -Force) | Out-Null
    $userCreated = $true
    $null = New-Item -Type Directory $fixture
    Copy-Item -LiteralPath $Installer -Destination (Join-Path $fixture 'setup.exe')
    Copy-Item -LiteralPath "$PSScriptRoot\qualify-installer.py","$PSScriptRoot\..\installed_deletion.py" -Destination $fixture
    $python = (Get-Command python.exe).Source
    $report = Join-Path $fixture 'result.txt'
    $taskScript = Join-Path $fixture 'run.ps1'
    @(
        "Set-Location '$fixture'",
        "& '$python' -u 'qualify-installer.py' 'setup.exe' *> '$report'",
        'exit $LASTEXITCODE'
    ) | Set-Content -LiteralPath $taskScript
    & icacls.exe $fixture /grant "${identity}:(OI)(CI)(M)" | Out-Null
    if ($LASTEXITCODE) { throw 'Could not grant the disposable test account access to its fixture' }
    $action = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument "-NoProfile -ExecutionPolicy RemoteSigned -File $taskScript"
    Register-ScheduledTask -TaskName $name -Action $action -User $identity -Password $password -RunLevel Limited | Out-Null
    $taskCreated = $true
    Start-ScheduledTask -TaskName $name
    $deadline = (Get-Date).AddMinutes(12)
    do {
        Start-Sleep -Seconds 2
        $state = (Get-ScheduledTask -TaskName $name).State
        $info = Get-ScheduledTaskInfo -TaskName $name
    } while (($state -eq 'Running' -or $info.LastRunTime.Year -lt 2000) -and (Get-Date) -lt $deadline)
    if (Test-Path -LiteralPath $report) { Get-Content -LiteralPath $report }
    if ($state -eq 'Running' -or $info.LastTaskResult -ne 0 -or !(Test-Path -LiteralPath $report)) { throw "Disposable-account verification failed with task result $($info.LastTaskResult)" }
} finally {
    if ($taskCreated) { Unregister-ScheduledTask -TaskName $name -Confirm:$false }
    if ($userCreated) { Remove-LocalUser -Name $name }
}
