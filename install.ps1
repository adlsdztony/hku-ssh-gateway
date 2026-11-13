[CmdletBinding()]
param(
    [string]$Version = 'latest',
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA 'Programs\hku'),
    [switch]$NoPathUpdate
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repository = 'adlsdztony/hku-ssh-gateway'
if ($env:OS -ne 'Windows_NT') { throw 'Use install.sh on Linux.' }
if ($env:PROCESSOR_ARCHITECTURE -ne 'AMD64' -and $env:PROCESSOR_ARCHITEW6432 -ne 'AMD64') {
    throw 'This installer currently supports Windows x64 only.'
}
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
$headers = @{ 'User-Agent' = 'hku-installer'; 'Accept' = 'application/vnd.github+json' }
if ($Version -eq 'latest') {
    $Version = (Invoke-RestMethod -Uri "https://api.github.com/repos/$repository/releases/latest" -Headers $headers).tag_name
}
if ($Version -notmatch '^v[0-9]+\.[0-9]+\.[0-9]+([-.][A-Za-z0-9.-]+)?$') { throw 'Invalid release version.' }
$asset = "hku-$Version-x86_64-pc-windows-msvc.zip"
$baseUrl = "https://github.com/$repository/releases/download/$Version"
$tempParent = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
$tempName = 'hku-install-' + [Guid]::NewGuid().ToString('N')
$tempRoot = [IO.Path]::GetFullPath((Join-Path $tempParent $tempName))
$staging = $null
try {
    New-Item -ItemType Directory -Path $tempRoot | Out-Null
    Write-Host "Downloading hku $Version (Windows x64)..."
    $archive = Join-Path $tempRoot $asset
    $checksumFile = Join-Path $tempRoot 'SHA256SUMS'
    Invoke-WebRequest -UseBasicParsing -Uri "$baseUrl/$asset" -OutFile $archive
    Invoke-WebRequest -UseBasicParsing -Uri "$baseUrl/SHA256SUMS" -OutFile $checksumFile
    $pattern = '^([a-fA-F0-9]{64})\s+\*?' + [regex]::Escape($asset) + '$'
    $expected = @(Get-Content -LiteralPath $checksumFile | ForEach-Object {
        if ($_ -match $pattern) { $Matches[1] }
    })
    if ($expected.Count -ne 1) { throw 'Release checksum is missing or ambiguous.' }
    $actual = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash
    if ($actual -ne $expected[0]) { throw 'Checksum mismatch; refusing to install.' }

    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::OpenRead($archive)
    $candidate = Join-Path $tempRoot 'hku.exe'
    try {
        if ($zip.Entries.Count -ne 1 -or $zip.Entries[0].FullName -ne 'hku.exe') {
            throw 'Release archive must contain only hku.exe.'
        }
        [IO.Compression.ZipFileExtensions]::ExtractToFile($zip.Entries[0], $candidate, $false)
    } finally { $zip.Dispose() }
    $reported = & $candidate --version
    if ($LASTEXITCODE -ne 0 -or $reported -ne "hku $($Version.Substring(1))") { throw 'Unexpected binary version.' }

    $InstallDir = [IO.Path]::GetFullPath($InstallDir)
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    $destination = Join-Path $InstallDir 'hku.exe'
    $staging = Join-Path $InstallDir ('.hku-install-' + [Guid]::NewGuid().ToString('N') + '.exe')
    Copy-Item -LiteralPath $candidate -Destination $staging
    Move-Item -LiteralPath $staging -Destination $destination -Force
    $staging = $null
    if (-not $NoPathUpdate) {
        $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
        $entries = @($userPath -split ';' | Where-Object { $_ })
        $alreadyPresent = @($entries | Where-Object {
            [Environment]::ExpandEnvironmentVariables($_).TrimEnd('\') -ieq $InstallDir.TrimEnd('\')
        }).Count -gt 0
        if (-not $alreadyPresent) {
            [Environment]::SetEnvironmentVariable('Path', (($entries + $InstallDir) -join ';'), 'User')
        }
        if (-not (@($env:Path -split ';') -contains $InstallDir)) { $env:Path = "$InstallDir;$env:Path" }
    }
    Write-Host "Installed $destination"
    Write-Host 'Next: hku ssh -s YOUR_GATEWAY user@internal-host'
    Write-Host 'Windows client mode needs OpenSSH, not Docker. New terminals inherit the updated PATH.'
} finally {
    if ($staging -and (Test-Path -LiteralPath $staging)) { Remove-Item -LiteralPath $staging -Force }
    # Only remove the exact temporary directory created by this invocation.
    $resolvedTemp = [IO.Path]::GetFullPath($tempRoot)
    if ($resolvedTemp.StartsWith($tempParent, [StringComparison]::OrdinalIgnoreCase) -and
        [IO.Path]::GetFileName($resolvedTemp) -eq $tempName -and
        (Test-Path -LiteralPath $resolvedTemp)) {
        Remove-Item -LiteralPath $resolvedTemp -Recurse -Force
    }
}
