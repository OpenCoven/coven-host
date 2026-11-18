# Build on Windows 11 with its inbox .NET Framework compiler and tar.exe.
param(
    [Parameter(Mandatory=$true)][string]$Package,
    [Parameter(Mandatory=$true)][string]$OutputDirectory
)
$ErrorActionPreference = 'Stop'
$archive = (Resolve-Path -LiteralPath $Package).ProviderPath
if ((Get-FileHash -Algorithm SHA256 -LiteralPath $archive).Hash -ne '840292358c75fed515452c285cee884b2bb0ab24ff997d671b1c98828091d1e8') { throw 'Unexpected Windows package SHA-256' }
$work = New-Item -ItemType Directory -Path (Join-Path $env:TEMP ([guid]::NewGuid()))
try {
    & tar.exe xf $archive -C $work.FullName package/bin/coven.exe package/LICENSE package/LICENSE-COVEN-HOST
    if ($LASTEXITCODE) { throw 'Package extraction failed' }
    $cli = Join-Path $work.FullName 'package/bin/coven.exe'
    if ((Get-FileHash -Algorithm SHA256 -LiteralPath $cli).Hash -ne '1e7a6e20030d01492e60607ec7b9916a26b4bdbc4ccc2c1594877fd50c5786cf') { throw 'Unexpected CLI SHA-256' }
    $compressed = Join-Path $work.FullName 'cli.gz'
    $inputFile = [IO.File]::OpenRead($cli)
    $outputFile = [IO.File]::Create($compressed)
    try {
        $zip = New-Object IO.Compression.GZipStream($outputFile, [IO.Compression.CompressionMode]::Compress)
        try { $inputFile.CopyTo($zip) } finally { $zip.Dispose() }
    } finally { $outputFile.Dispose(); $inputFile.Dispose() }
    $destination = New-Item -Force -Type Directory -Path $OutputDirectory
    $out = Join-Path $destination.FullName 'Coven-Host-0.1.1-Windows-x64.exe'
    $compiler = Join-Path $env:WINDIR 'Microsoft.NET\Framework\v4.0.30319\csc.exe'
    & $compiler /nologo /target:winexe /platform:anycpu /optimize+ "/win32manifest:$PSScriptRoot\app.manifest" "/out:$out" "/resource:$compressed,cli.gz" "/resource:$work\package\LICENSE,license-cli.txt" "/resource:$work\package\LICENSE-COVEN-HOST,license-host.txt" /reference:System.Windows.Forms.dll /reference:System.Management.dll /reference:System.Drawing.dll /reference:System.Web.Extensions.dll /reference:Microsoft.CSharp.dll "$PSScriptRoot\Setup.cs"
    if ($LASTEXITCODE) { throw 'Compilation failed' }
    Get-FileHash -Algorithm SHA256 -LiteralPath $out
} finally { Remove-Item -Recurse -Force -LiteralPath $work.FullName }
