# ================================================================
# MetaAgent Float · 开发环境激活脚本
# 用法：在 PowerShell 中执行  . .\tools\activate-dev-env.ps1
# 作用：把 D:\DevelopToolsEnv 下的工具链注入当前会话 PATH
# 合规：所有路径均指向 D 盘，不修改注册表，不写 C 盘
# ================================================================

$ErrorActionPreference = 'Stop'

# ---- 工具链根目录（全部在 D 盘） ----
$env:DEV_TOOLS_ROOT   = 'D:\DevelopToolsEnv'
$env:RUSTUP_HOME      = "$env:DEV_TOOLS_ROOT\rustup"
$env:CARGO_HOME       = "$env:DEV_TOOLS_ROOT\cargo"
$env:NODE_HOME        = "$env:DEV_TOOLS_ROOT\nodejs"
$env:NODE_GLOBAL      = "$env:DEV_TOOLS_ROOT\nodejs-global"
$env:PNPM_STORE       = "$env:DEV_TOOLS_ROOT\pnpm-store"
$env:NPM_CACHE        = "$env:DEV_TOOLS_ROOT\npm-cache"

# ---- 测试数据根目录 ----
$env:TEST_DATA        = 'D:\MyProject\Datas'

# ---- PATH 注入（优先级最高，放最前） ----
$pathsToPrepend = @(
    "$env:CARGO_HOME\bin",
    "$env:NODE_HOME",
    "$env:NODE_GLOBAL",
    "$env:NODE_HOME\node_modules\corepack\shims"
)
foreach ($p in $pathsToPrepend) {
    if (Test-Path $p) {
        if ($env:PATH -notlike "*$p*") {
            $env:PATH = "$p;$env:PATH"
        }
    }
}

# ---- 镜像与缓存（全部指向 D 盘 / 国内镜像） ----
$env:NPM_CONFIG_REGISTRY    = 'https://registry.npmmirror.com'
$env:NPM_CONFIG_CACHE       = $env:NPM_CACHE
$env:NPM_CONFIG_PREFIX      = $env:NODE_GLOBAL
$env:PNPM_HOME              = $env:NODE_GLOBAL
$env:CARGO_HTTP_MULTIPLEXING = 'false'   # 国内网络更稳

# ---- 打印激活结果 ----
Write-Host ''
Write-Host '===============================================' -ForegroundColor Cyan
Write-Host '  MetaAgent Float 开发环境已激活' -ForegroundColor Green
Write-Host '===============================================' -ForegroundColor Cyan
Write-Host "  RUSTUP_HOME  = $env:RUSTUP_HOME"
Write-Host "  CARGO_HOME   = $env:CARGO_HOME"
Write-Host "  NODE_HOME    = $env:NODE_HOME"
Write-Host "  PNPM_STORE   = $env:PNPM_STORE"
Write-Host "  TEST_DATA    = $env:TEST_DATA"
Write-Host '-----------------------------------------------' -ForegroundColor Cyan

# ---- 工具链自检（有就显示版本，没有就警告） ----
function Show-Version($name, $cmd, $arg = '--version') {
    try {
        $v = & $cmd $arg 2>&1 | Select-Object -First 1
        Write-Host ("  {0,-12} : {1}" -f $name, $v) -ForegroundColor Green
    } catch {
        Write-Host ("  {0,-12} : NOT FOUND" -f $name) -ForegroundColor Yellow
    }
}
Show-Version 'rustc'      'rustc'
Show-Version 'cargo'      'cargo'
Show-Version 'node'       'node'
Show-Version 'npm'        'npm'
# pnpm 走 node 直调（绕过 corepack 签名 bug）
try {
    $pnpmV = & node 'D:\DevelopToolsEnv\nodejs-global\node_modules\pnpm\bin\pnpm.mjs' --version 2>&1 | Select-Object -First 1
    Write-Host ("  {0,-12} : {1}" -f 'pnpm', $pnpmV) -ForegroundColor Green
} catch {
    Write-Host ("  {0,-12} : NOT FOUND" -f 'pnpm') -ForegroundColor Yellow
}
# tauri-cli 走 pnpm 装的 @tauri-apps/cli（项目内 node_modules）
if (Test-Path '.\node_modules\@tauri-apps\cli\tauri.js') {
    try {
        $tv = & node '.\node_modules\@tauri-apps\cli\tauri.js' --version 2>&1 | Select-Object -First 1
        Write-Host ("  {0,-12} : {1}" -f 'tauri-cli', $tv) -ForegroundColor Green
    } catch {
        Write-Host ("  {0,-12} : ERROR" -f 'tauri-cli') -ForegroundColor Yellow
    }
} else {
    Write-Host ("  {0,-12} : run 'pnpm i' first" -f 'tauri-cli') -ForegroundColor Yellow
}
Write-Host '===============================================' -ForegroundColor Cyan
Write-Host ''
