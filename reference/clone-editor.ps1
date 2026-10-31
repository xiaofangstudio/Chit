<#
  下载 Chit 编辑器底座（只读参考，绝不提交）
  用法：
    .\clone-editor.ps1
    .\clone-editor.ps1 -Proxy http://127.0.0.1:7890
#>
param([string]$Proxy = "")

$repos = @(
  @{ Name = "notepad4"; Url = "https://github.com/zufuliu/notepad4.git" }
)

$root = Join-Path (Split-Path -Parent $MyInvocation.MyCommand.Path) "editor"
New-Item -ItemType Directory -Force -Path $root | Out-Null

$ok = 0
$fail = @()

foreach ($r in $repos) {
  $dest = Join-Path $root $r.Name
  if (Test-Path $dest) { Write-Host "[跳过] $($r.Name) 已存在"; $ok++; continue }
  Write-Host "[克隆] $($r.Name)"
  $gitArgs = @("clone", "--depth", "1")
  if ($Proxy) { $gitArgs += @("-c", "http.proxy=$Proxy") }
  $gitArgs += @($r.Url, $dest)
  & git @gitArgs
  if (Test-Path $dest) { $ok++ } else { $fail += $r.Name; Remove-Item -Recurse -Force $dest -ErrorAction SilentlyContinue }
}

Write-Host ""
Write-Host "成功 $ok / $($repos.Count)"
if ($fail.Count -gt 0) { Write-Host "失败：$($fail -join ', ')" }
Write-Host "未执行任何 git add / commit / push。"