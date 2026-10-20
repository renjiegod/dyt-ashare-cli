[CmdletBinding()]
param([string]$ManifestUrl = 'https://data.doyoutrade.cloud/releases/dyt-cn/stable.json', [switch]$NoSchedule, [switch]$NoSkills)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
# TLS authenticates first installation. All later releases are verified by the embedded Ed25519 key.
$uri = [Uri]$ManifestUrl
if ($uri.Scheme -ne 'https' -and -not $uri.IsLoopback) { throw 'HTTPS is required' }
$root = if ($env:DYT_CN_HOME) { $env:DYT_CN_HOME } else { Join-Path $env:LOCALAPPDATA 'DYT\CN' }
$env:DYT_CN_HOME = $root
$previousChild = $env:DYT_CN_CHILD
$arch = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
$target = switch ($arch) { 'X64' {'x86_64-pc-windows-msvc'} 'Arm64' {'aarch64-pc-windows-msvc'} default {throw "Unsupported architecture: $arch"} }
# A target-specific manifest avoids downloading any unrelated platform binaries.
if ($ManifestUrl.EndsWith('/stable.json')) { $ManifestUrl = $ManifestUrl.Replace('/stable.json', "/stable-$target.json") }
$temp = Join-Path ([IO.Path]::GetTempPath()) ('dyt-cn-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $temp | Out-Null
try {
    $manifestPath = Join-Path $temp 'release.json'
    Invoke-WebRequest -UseBasicParsing -Uri $ManifestUrl -OutFile $manifestPath -TimeoutSec 30 -MaximumRedirection 0
    $envelope = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    $manifest = $envelope.payload | ConvertFrom-Json
    if ($manifest.target -ne $target) { throw 'Manifest target mismatch' }
    $asset = @($manifest.files | Where-Object path -eq 'dyt-cn.exe')
    if ($asset.Count -ne 1 -or $asset[0].bytes -gt 134217728) { throw 'Invalid bootstrap asset' }
    $assetUri = [Uri]$asset[0].url
    if ($assetUri.Scheme -ne 'https' -and -not $assetUri.IsLoopback) { throw 'HTTPS is required' }
    $bootstrap = Join-Path $temp 'dyt-cn.exe'
    Invoke-WebRequest -UseBasicParsing -Uri $assetUri -OutFile $bootstrap -TimeoutSec 300 -MaximumRedirection 0
    if ((Get-Item -LiteralPath $bootstrap).Length -ne $asset[0].bytes -or (Get-FileHash -LiteralPath $bootstrap -Algorithm SHA256).Hash.ToLowerInvariant() -ne $asset[0].sha256) { throw 'Bootstrap size/SHA256 mismatch' }
    $env:DYT_CN_CHILD = '1'
    & $bootstrap update --manifest $manifestPath
    if ($LASTEXITCODE -ne 0) { throw 'Signed bundle installation failed' }
    Remove-Item Env:DYT_CN_CHILD
    $cli = Join-Path $root 'bin\dyt-cn.exe'
    if (-not $NoSkills) { & $cli skills install; if ($LASTEXITCODE -ne 0) {throw 'Skills installation failed; rerun dyt-cn skills install'} }
    $bin = Join-Path $root 'bin'
    $userPath = [Environment]::GetEnvironmentVariable('Path','User')
    if (@($userPath -split ';') -notcontains $bin) { [Environment]::SetEnvironmentVariable('Path', "$bin;$userPath", 'User') }
    if (@($env:Path -split ';') -notcontains $bin) { $env:Path = "$bin;$env:Path" }
    if (-not $NoSchedule) {
        $hasher = [Security.Cryptography.SHA256]::Create()
        try { $taskId = [BitConverter]::ToString($hasher.ComputeHash([Text.Encoding]::UTF8.GetBytes($root))).Replace('-','').Substring(0,12) } finally { $hasher.Dispose() }
        $taskName = 'DYT CN Auto Update ' + $taskId
        $action = New-ScheduledTaskAction -Execute $cli -Argument "--home `"$root`" update --background"
        $trigger = New-ScheduledTaskTrigger -Once -At (Get-Date).AddMinutes(5) -RepetitionInterval (New-TimeSpan -Hours 6)
        $settings = New-ScheduledTaskSettingsSet -StartWhenAvailable -MultipleInstances IgnoreNew -ExecutionTimeLimit (New-TimeSpan -Minutes 15)
        Register-ScheduledTask -TaskName $taskName -Action $action -Trigger $trigger -Settings $settings -Description 'DYT CN signed automatic updates for this user' -Force | Out-Null
        @{name=$taskName; enabled=$true} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $root 'scheduled-task.json') -Encoding UTF8
    }
    Write-Host 'DYT CN installed. Run: dyt-cn auth login'
} finally { $env:DYT_CN_CHILD = $previousChild; Remove-Item -LiteralPath $temp -Recurse -Force -ErrorAction SilentlyContinue }
