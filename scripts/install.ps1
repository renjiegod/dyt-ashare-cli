[CmdletBinding()]
param([string]$ManifestUrl = 'https://data.doyoutrade.cloud/releases/dyt-ashare/channel.json', [switch]$NoSchedule, [switch]$NoSkills)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
# PowerShell 7 can pass a module search path that omits Windows PowerShell's inbox modules.
$windowsModules = Join-Path $env:WINDIR 'System32\WindowsPowerShell\v1.0\Modules'
if (@($env:PSModulePath -split ';') -notcontains $windowsModules) { $env:PSModulePath = "$windowsModules;$env:PSModulePath" }
function Get-Sha256([string]$Path) {
    $stream = [IO.File]::OpenRead($Path)
    $hasher = [Security.Cryptography.SHA256]::Create()
    try { return [BitConverter]::ToString($hasher.ComputeHash($stream)).Replace('-','').ToLowerInvariant() }
    finally { $stream.Dispose(); $hasher.Dispose() }
}
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
# TLS authenticates the channel file. The CLI verifies the repository anchor
# before it installs anything.
$uri = [Uri]$ManifestUrl
if ($uri.Scheme -ne 'https' -and -not $uri.IsLoopback) { throw 'HTTPS is required' }
$root = if ($env:DYT_ASHARE_HOME) { $env:DYT_ASHARE_HOME } else { Join-Path $env:LOCALAPPDATA 'DYT\AShare' }
$env:DYT_ASHARE_HOME = $root
$previousChild = $env:DYT_ASHARE_CHILD
$arch = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
$target = switch ($arch) { 'X64' {'x86_64-pc-windows-msvc'} 'Arm64' {'aarch64-pc-windows-msvc'} default {throw "Unsupported architecture: $arch"} }
# A target-specific manifest avoids downloading any unrelated platform binaries.
if ($ManifestUrl.EndsWith('/channel.json')) { $ManifestUrl = $ManifestUrl.Replace('/channel.json', "/channel-$target.json") }
$temp = Join-Path ([IO.Path]::GetTempPath()) ('dyt-ashare-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $temp | Out-Null
try {
    $manifestPath = Join-Path $temp 'release.json'
    Invoke-WebRequest -UseBasicParsing -Uri $ManifestUrl -OutFile $manifestPath -TimeoutSec 30 -MaximumRedirection 0
    $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    if ($manifest.target -ne $target) { throw 'Manifest target mismatch' }
    $asset = @($manifest.files | Where-Object path -eq 'dytcn.exe')
    if ($asset.Count -ne 1 -or $asset[0].bytes -gt 134217728) { throw 'Invalid bootstrap asset' }
    $assetUri = [Uri]$asset[0].url
    if ($assetUri.Scheme -ne 'https' -and -not $assetUri.IsLoopback) { throw 'HTTPS is required' }
    $bootstrap = Join-Path $temp 'dytcn.exe'
    Invoke-WebRequest -UseBasicParsing -Uri $assetUri -OutFile $bootstrap -TimeoutSec 300 -MaximumRedirection 0
    if ((Get-Item -LiteralPath $bootstrap).Length -ne $asset[0].bytes -or (Get-Sha256 $bootstrap) -ne $asset[0].sha256) { throw 'Bootstrap size/SHA256 mismatch' }
    $env:DYT_ASHARE_CHILD = '1'
    & $bootstrap update --manifest $manifestPath
    if ($LASTEXITCODE -ne 0) { throw 'Channel bundle installation failed' }
    Remove-Item Env:DYT_ASHARE_CHILD
    $cli = Join-Path $root 'bin\dytcn.exe'
    if (-not $NoSkills) { & $cli skills install; if ($LASTEXITCODE -ne 0) {throw 'Skills installation failed; rerun dytcn skills install'} }
    $bin = Join-Path $root 'bin'
    $userPath = [Environment]::GetEnvironmentVariable('Path','User')
    if (@($userPath -split ';') -notcontains $bin) { [Environment]::SetEnvironmentVariable('Path', "$bin;$userPath", 'User') }
    if (@($env:Path -split ';') -notcontains $bin) { $env:Path = "$bin;$env:Path" }
    if (-not $NoSchedule) {
        $hasher = [Security.Cryptography.SHA256]::Create()
        try { $taskId = [BitConverter]::ToString($hasher.ComputeHash([Text.Encoding]::UTF8.GetBytes($root))).Replace('-','').Substring(0,12) } finally { $hasher.Dispose() }
        $taskName = 'DYT AShare Auto Update ' + $taskId
        $action = New-ScheduledTaskAction -Execute $cli -Argument "--home `"$root`" update --background"
        $trigger = New-ScheduledTaskTrigger -Once -At (Get-Date).AddMinutes(5) -RepetitionInterval (New-TimeSpan -Hours 6)
        $settings = New-ScheduledTaskSettingsSet -StartWhenAvailable -MultipleInstances IgnoreNew -ExecutionTimeLimit (New-TimeSpan -Minutes 15)
        Register-ScheduledTask -TaskName $taskName -Action $action -Trigger $trigger -Settings $settings -Description 'DYT AShare automatic updates for this user' -Force | Out-Null
        @{name=$taskName; enabled=$true} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $root 'scheduled-task.json') -Encoding UTF8
    }
    Write-Host 'DYT AShare installed. Run: dytcn auth login'
} finally { $env:DYT_ASHARE_CHILD = $previousChild; Remove-Item -LiteralPath $temp -Recurse -Force -ErrorAction SilentlyContinue }
