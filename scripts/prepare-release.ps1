param([string]$VCRedistDirectory = $env:VCTOOLS_REDIST_DIR)

$ErrorActionPreference = 'Stop'
$releaseRoot = Split-Path -Parent $PSScriptRoot
if (-not $VCRedistDirectory) {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    if (-not (Test-Path -LiteralPath $vswhere -PathType Leaf)) {
        throw 'Visual Studio Build Tools nao encontrado. Informe VCTOOLS_REDIST_DIR na maquina de build.'
    }
    $vsRoot = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if (-not $vsRoot) { throw 'C++ Build Tools x64 nao encontrado.' }
    $redistRoot = Join-Path $vsRoot 'VC/Redist/MSVC'
    $VCRedistDirectory = $redistRoot
}
$VCRedistDirectory = (Resolve-Path -LiteralPath $VCRedistDirectory).Path
if (-not (Test-Path -LiteralPath (Join-Path $VCRedistDirectory 'x64') -PathType Container)) {
    $selected = Get-ChildItem -LiteralPath $VCRedistDirectory -Directory |
        Where-Object { $_.Name -match '^\d+\.\d+\.\d+$' } |
        Sort-Object { [version]$_.Name } -Descending | Select-Object -First 1
    if (-not $selected) { throw 'Redist MSVC x64 nao encontrado.' }
    $VCRedistDirectory = $selected.FullName
}
$crt = Join-Path $VCRedistDirectory 'x64/Microsoft.VC143.CRT'
$openmp = Join-Path $VCRedistDirectory 'x64/Microsoft.VC143.OpenMP/vcomp140.dll'
$runtimeNames = @('msvcp140.dll', 'msvcp140_1.dll', 'vcruntime140.dll', 'vcruntime140_1.dll')
$runtimeSources = @($runtimeNames | ForEach-Object { Join-Path $crt $_ }) + @($openmp)
foreach ($source in $runtimeSources) {
    if (-not (Test-Path -LiteralPath $source -PathType Leaf)) { throw "Runtime obrigatorio ausente: $source" }
}
# Copy redistributable originals, never DLLs from System32. Each native process has
# its own executable directory in Windows' DLL search order.
foreach ($module in @('whisper', 'diarization')) {
    $destination = Join-Path $releaseRoot "src-tauri/resources/$module"
    foreach ($source in $runtimeSources) {
        Copy-Item -LiteralPath $source -Destination $destination -Force
    }
}
$resources = Join-Path $releaseRoot 'src-tauri/resources'
foreach ($required in @(
    'whisper/whisper-cli.exe', 'whisper/ggml-base-q5_1.bin', 'whisper/whisper.dll',
    'diarization/sherpa-onnx-offline-speaker-diarization.exe', 'diarization/onnxruntime.dll',
    'diarization/segmentation.int8.onnx', 'diarization/wespeaker_resnet34_lm.onnx'
)) {
    if (-not (Test-Path -LiteralPath (Join-Path $resources $required) -PathType Leaf)) {
        throw "Recurso local obrigatorio ausente: $required"
    }
}
Write-Host "Runtimes x64 preparados: $($runtimeSources.Count) DLLs por processador."
Push-Location $releaseRoot
try {
    & npm.cmd run build
    if ($LASTEXITCODE -ne 0) { throw "Frontend build falhou: $LASTEXITCODE" }
} finally { Pop-Location }
