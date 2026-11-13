param([Parameter(Mandatory = $true)][string]$Binary)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$parent = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
$name = 'hku-installer-test-' + [Guid]::NewGuid().ToString('N')
$temporary = [IO.Path]::GetFullPath((Join-Path $parent $name))
New-Item -ItemType Directory -Path $temporary | Out-Null
try {
    $version = (& $Binary --version) -replace '^hku ', 'v'
    $fixture = Join-Path $temporary 'fixture'
    $env:HKU_INSTALLER_TEST_VERSION = $version
    $env:HKU_INSTALLER_TEST_FIXTURE = $fixture
    New-Item -ItemType Directory -Path $fixture | Out-Null
    $asset = "hku-$version-x86_64-pc-windows-msvc.zip"
    $archive = Join-Path $fixture $asset
    Compress-Archive -LiteralPath $Binary -DestinationPath $archive
    $hash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash
    [IO.File]::WriteAllText((Join-Path $fixture 'SHA256SUMS'), "$hash  $asset`n")
    function Invoke-RestMethod {
        param($Uri, $Headers)
        [pscustomobject]@{ tag_name = $env:HKU_INSTALLER_TEST_VERSION }
    }
    function Invoke-WebRequest {
        param([switch]$UseBasicParsing, $Uri, $OutFile)
        Copy-Item -LiteralPath (Join-Path $env:HKU_INSTALLER_TEST_FIXTURE ($Uri.Split('/')[-1])) -Destination $OutFile
    }
    $installDir = Join-Path $temporary 'bin'
    & (Join-Path $root 'install.ps1') -InstallDir $installDir -NoPathUpdate
    $installed = Join-Path $installDir 'hku.exe'
    if ((& $installed --version) -ne "hku $($version.Substring(1))") { throw 'Incorrect installed binary.' }
    $before = (Get-FileHash -LiteralPath $installed -Algorithm SHA256).Hash
    Add-Content -LiteralPath $archive -Value 'corruption' -NoNewline
    $rejected = $false
    try { & (Join-Path $root 'install.ps1') -Version $version -InstallDir $installDir -NoPathUpdate }
    catch { if ($_.Exception.Message -like '*Checksum mismatch*') { $rejected = $true } else { throw } }
    if (-not $rejected) { throw 'Corrupted archive was accepted.' }
    if ((Get-FileHash -LiteralPath $installed -Algorithm SHA256).Hash -ne $before) { throw 'Existing binary was replaced after failed verification.' }
    Write-Host 'PASS: Windows installer, latest version, checksum rejection and existing binary preservation.'
} finally {
    Remove-Item Env:HKU_INSTALLER_TEST_VERSION -ErrorAction SilentlyContinue
    Remove-Item Env:HKU_INSTALLER_TEST_FIXTURE -ErrorAction SilentlyContinue
    $resolved = [IO.Path]::GetFullPath($temporary)
    if ($resolved.StartsWith($parent, [StringComparison]::OrdinalIgnoreCase) -and
        [IO.Path]::GetFileName($resolved) -eq $name) {
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}
