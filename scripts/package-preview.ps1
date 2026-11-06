[CmdletBinding()]
param([string]$PackageName = 'SwitcherPet-preview-20261106-1')
$ErrorActionPreference = 'Stop'
if ($PackageName -notmatch '^SwitcherPet-preview-[0-9-]+$') { throw 'Invalid preview package name.' }
$repo = Split-Path -Parent $PSScriptRoot
$releaseRoot = Join-Path (Split-Path -Parent $repo) 'releases'
$destination = Join-Path $releaseRoot $PackageName
if ((Test-Path -LiteralPath $destination) -or (Test-Path -LiteralPath ($destination + '.zip'))) {
    throw 'Package already exists; choose a new revision instead of overwriting it.'
}
$exe = Join-Path $repo 'src-tauri/target/release/switcher-pet.exe'
& node (Join-Path $repo 'scripts/verify-windows-artifact.mjs') $exe
if ($LASTEXITCODE -ne 0) { throw 'Cannot package an invalid binary.' }
[void][IO.Directory]::CreateDirectory($destination)
foreach ($file in @('switcher-pet.exe','WebView2Loader.dll')) {
    Copy-Item -LiteralPath (Join-Path (Split-Path -Parent $exe) $file) -Destination $destination
}
Copy-Item -LiteralPath (Join-Path $repo 'LICENSE') -Destination (Join-Path $destination 'LICENSE-upstream.txt')
Copy-Item -LiteralPath (Join-Path $repo 'public/pets/LICENSE-CC-BY-NC-4.0.txt') -Destination (Join-Path $destination 'LICENSE-pet-assets.txt')
Copy-Item -LiteralPath (Join-Path $repo 'public/pets/NOTICE.md') -Destination (Join-Path $destination 'NOTICE-pet-assets.md')
Copy-Item -LiteralPath (Join-Path $repo 'public/pets/manifest.json') -Destination (Join-Path $destination 'pet-assets-manifest.json')
[IO.File]::WriteAllText((Join-Path $destination 'Launch.ps1'), [IO.File]::ReadAllText((Join-Path $repo 'scripts/launch-preview.ps1')), [Text.UTF8Encoding]::new($true))
[IO.File]::WriteAllText((Join-Path $destination 'Open-preview.cmd'), "@echo off`r`npowershell.exe -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File `"%~dp0Launch.ps1`"`r`n", [Text.ASCIIEncoding]::new())
[IO.File]::WriteAllText((Join-Path $destination 'Open-settings.cmd'), "@echo off`r`npowershell.exe -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File `"%~dp0Launch.ps1`" -Settings`r`n", [Text.ASCIIEncoding]::new())
$readme = @'
Switcher Pet — 自用开源版

preview 18：捏桌宠会主动刷新额度，并发动作合并；刷新失败保留旧余额。自动来源支持精确同模型的 Responses 和 Chat Completions API，不再因 Chat 协议拒绝勾选；保留克劳德比例及 Google 修复。macOS arm64 另行构建，内置菜单栏用量监控。此 Windows 包不含账号、密钥或个人配置。

API余额在简约模式和详细模式均显示实际金额与币种。详细模式以余额金额为主数字，API额度条也标注金额，余额百分比为次要信息；订阅五小时/周号池继续使用可叠加的等效百分比。

本次修复：模型目录补全Codex必需字段，并用本机Codex的model/list实际验证。Gemini各思考预设合成一个模型，low/medium/high独立选择并匹配选定来源的变体。Business共享工作区的不同用户席位分别计入号池；两个满额1×席位显示≈200%。额度条标注也使用可叠加的大数，填充长度为剩余/总容量。旧额度保留并标记为上次记录；部分未知账号不再让已知额度条消失，详细模式补上API余额条。

双击 Open-preview.cmd 打开；本机有待迁移配置时，直接双击 switcher-pet.exe 也会自动进入独立预览。
必须将 WebView2Loader.dll 保留在程序旁边。需要 Windows WebView2 Runtime（本机已验证可用）。
不需要安装、不覆盖原版、不接管原版代理、不注册原版链接。

预览账号目录：%USERPROFILE%\.switcher-pet\preview\profile
已复制的账号仍在：%USERPROFILE%\.switcher-pet\profile
原始加密备份仍在：%USERPROFILE%\.switcher-pet\imports
此便携包不含账号、密钥、配置备份或聊天记录。
不要删除 import-pending.json 来强行启用账号；刷新令牌所有权尚未迁移。

桌宠用量改为简洁小组件：当前模型的可用API余额按币种合并，底部总池按剩余/总容量显示进度；隐藏来源与观察器信息，恢复捏一捏不连重复的随机对话气泡。API未提供总额度时，以首次余额/充值峰值作持久估算基准，显示≈；可在设置修改基准。
五小时与周额度按全池订阅统计，不会因为当前模型切到Google/API而被该模型自己的额度遮住；若全池确认没有受限账号，则只显示周额度。模型目录每15秒自动读取一次最新目录，切回应用窗口时也立即刷新。
本次修复原生账号批量导入丢失类型/URL/路由的错误；API不再按订阅过期判断。模型探针补齐裸域名/v1路径，手动探测可恢复误标账号，即使自动开关关闭也实际请求。探针结果及限流冷却持久化，余额失败不伪造金额。
本版包含：统一模型与来源页，Google、OpenAI订阅/API Key和第三方API同模型合并。原生模型顺序读取本机Codex目录；选择作用于真实HTTP/WS请求，不改全局身份。
原生模型与第三方API现在共用逻辑模型ID：同名直接合并；名称不同时使用来源中的“模型映射”（左侧客户端模型名、右侧API实际模型名）。只有转发到来源时改写模型名；桌宠余额、来源选择与自动切换均按逻辑模型归并。映射后的API内部名称不再重复占一个目录选项，老会话relay-current别名仍兼容。
模型名旁的星空百分比条优先五小时、其次周额度；长按650ms切换简约/详细，展开收回平滑过渡，减少动画设置可禁用动效。代理标准端口18080，原版占用时新版保持关闭。
新增模型目录隐藏/禁止新请求，探针刷新不会覆盖选择。
新增额度与探针页：可编辑Plus等效倍率、独立账号/总池、五小时受限子池与独立周额度、共享余额去重、分币种余额与预算。
API只读探针逐来源配置；新API来源默认开启模型探测和自动余额识别，不发聊天或工具，不刷新OAuth，遵循限流冷却，不跟随重定向。
默认静默启动，只显示桌宠；右键“打开设置”进入新的桌宠与声音页。可在此关闭静默启动。捏一捏模式按住变形、松开回弹，用下方移动按钮拖动位置；关闭此模式可直接拖动形象。
桌宠使用8套fywdy同人角色的未修改动画，CC BY-NC 4.0，仅非商业使用；许可、声明和素材清单随包。
按下/松开音效由本地MP3/WAV加载，可导入、试听、调音量和静音；不再合成替代音。本机已配置的鲸鱼娘原音保存在自用profile，不随此包分发。
自动换装开关同时出现在右键菜单和桌宠与声音页；读取本机Codex主对话模型元信息（只读），并接收代理请求事件。以最新主回合为准，不代表前台窗口焦点；明确子任务和探针不抢形象，可固定形象。
原生Responses压缩、指定账号优先级、子任务HTTP/WS绑定经过本机合成上游端到端验收。
自动切换覆盖OpenAI订阅、API Key、原生Responses及Google来源：明确429、流开始前、同模型、显式勾选来源；不降级。
工具结果图片与结构化数据、Google图片编码、GPT OSS默认推理和输出上限已补兼容回归测试。
Google的HTTP/WS账号硬绑定、模型开关和同模型切换有合成上游验收。Gemini 3 Flash与Claude工具调用已直连验证；Google GPT OSS带工具返回上游模板错误，Gemini 3.8实测超时，不宣称已修好。
Chat Completions不支持原生压缩时明确返回501，不伪装成限流；跨协议压缩仍待实现。
导入的Google身份命中原版同一账号时，只读取原版已有访问令牌，不刷新、不写回原版；原版凭据过期时明确报错。其他OAuth账号仍不可双刷新。

关闭窗口会收起到新版自己的托盘。
如果旧版独立预览正在运行，新预览不会抢占其配置；退出旧预览后再打开新版。不要退出原版Codex Switcher。
上游源码：VallierDev/codex-switcher（MIT，见附带LICENSE）。
WebView2Loader及运行时遵循微软对应许可，不因上游MIT许可而改变。
'@
[IO.File]::WriteAllText((Join-Path $destination '使用说明.txt'), $readme, [Text.UTF8Encoding]::new($true))
$hashes = Get-ChildItem -LiteralPath $destination -File | ForEach-Object {
    [PSCustomObject]@{File=$_.Name; SHA256=(Get-FileHash -LiteralPath $_.FullName).Hash}
}
[IO.File]::WriteAllText((Join-Path $destination 'SHA256.json'), ($hashes | ConvertTo-Json), [Text.UTF8Encoding]::new($false))
Compress-Archive -Path (Join-Path $destination '*') -DestinationPath ($destination + '.zip')
Write-Output "Preview package: $destination"
Write-Output "ZIP: $destination.zip"
