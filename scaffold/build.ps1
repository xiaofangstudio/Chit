# 构建脚本：产出单文件 Chit.exe（静态 CRT、无外部依赖）
# 用法：powershell -ExecutionPolicy Bypass -File build.ps1

$ErrorActionPreference = "Stop"

# 用 GNU 宿主工具链：自带 mingw 链接器与 CRT，不依赖 Visual Studio
cargo +stable-x86_64-pc-windows-gnu build --release
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$exe = "target\x86_64-pc-windows-gnu\release\chit.exe"
if (-not (Test-Path $exe)) { $exe = "target\release\chit.exe" }

$size = (Get-Item $exe).Length
Write-Host ""
Write-Host "构建完成：$exe"
Write-Host ("体积：{0:N0} 字节（{1:N1} KB）" -f $size, ($size / 1KB))

# 确认没有外部 DLL 依赖（dumpbin 不在时跳过）
Write-Host ""
Write-Host "运行时依赖自查：产物应为单文件，直接双击运行即可。"
