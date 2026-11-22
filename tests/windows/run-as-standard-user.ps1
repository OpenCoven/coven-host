# Run only on a disposable Windows Actions host. The account and process belong to this run.
param([Parameter(Mandatory=$true)][string]$Installer)
$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_OS -ne 'Windows') { throw 'A disposable Windows Actions runner is required' }
$name = 'CovenQA' + [Guid]::NewGuid().ToString('N').Substring(0,8)
$fixture = Join-Path $env:SystemDrive $name
$password = [Guid]::NewGuid().ToString('N') + 'aA2!'
$identity = "$env:COMPUTERNAME\$name"
$userCreated = $false
$testProcess = $null
try {
    New-LocalUser -Name $name -Password (ConvertTo-SecureString -String $password -AsPlainText -Force) | Out-Null
    $userCreated = $true
    $users = Get-LocalGroup -SID 'S-1-5-32-545'
    if (!(Get-LocalGroupMember -Group $users | Where-Object SID -eq (Get-LocalUser -Name $name).SID)) {
        Add-LocalGroupMember -Group $users -Member $name
    }
    $null = New-Item -Type Directory $fixture
    Copy-Item -LiteralPath $Installer -Destination (Join-Path $fixture 'setup.exe')
    Copy-Item -LiteralPath "$PSScriptRoot\qualify-installer.py","$PSScriptRoot\..\installed_deletion.py" -Destination $fixture
    $python = (Get-Command python.exe).Source
    $report = Join-Path $fixture 'result.txt'
    $taskScript = Join-Path $fixture 'run.ps1'
    $exitFile = Join-Path $fixture 'exit.txt'
    @(
        "Set-Location '$fixture'",
        "& '$python' -u 'qualify-installer.py' 'setup.exe' *> '$report'",
        '$result = $LASTEXITCODE',
        ("Set-Content -LiteralPath '{0}' -Value `$result" -f $exitFile),
        'exit $result'
    ) | Set-Content -LiteralPath $taskScript
    & icacls.exe $fixture /grant "${identity}:(OI)(CI)(M)" | Out-Null
    if ($LASTEXITCODE) { throw 'Could not grant the disposable test account access to its fixture' }
    $credential = [PSCredential]::new($identity, (ConvertTo-SecureString $password -AsPlainText -Force))
    Start-Service seclogon
    $testProcess = Start-Process -FilePath "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" -ArgumentList "-NoProfile -ExecutionPolicy RemoteSigned -File $taskScript" -Credential $credential -LoadUserProfile -WorkingDirectory $fixture -PassThru
    if (!$testProcess.WaitForExit(300000)) { throw 'The standard-account test did not finish within five minutes' }
    if (Test-Path -LiteralPath $report) { Get-Content -LiteralPath $report }
    if ($testProcess.ExitCode -ne 0 -or !(Test-Path -LiteralPath $exitFile) -or (Get-Content -LiteralPath $exitFile -Raw).Trim() -ne '0') {
        throw "Disposable-account verification failed with process exit $($testProcess.ExitCode)"
    }
} finally {
    if ($testProcess -and !$testProcess.HasExited) { $testProcess.Kill() }
    if ($userCreated) { Remove-LocalUser -Name $name }
}
