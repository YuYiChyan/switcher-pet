# Switcher Pet — 自用开源版

基于 [VallierDev/codex-switcher](https://github.com/VallierDev/codex-switcher) 的独立桌宠 fork。Windows 便携预览使用独立应用标识、账号目录和更新渠道；原版安装、账号、Codex 身份配置保持独立。

## 使用

从本仓库 Releases 下载便携 ZIP，解压后打开 `Open-preview.cmd`。右键桌宠打开设置。需要 Windows WebView2 Runtime；请保留程序旁的 `WebView2Loader.dll`。配置保存在 `%USERPROFILE%\.switcher-pet\preview\profile`，不随发行包分发。

macOS 仅构建 Apple Silicon（arm64），不提供 Intel 包。安装与内置菜单栏额度监控见 [macOS 说明](README.macOS.md)。

默认代理端口 18080。原版占用端口时先在设置中给新版选择独立端口；不要为新版终止原版。导入 OAuth 账号须确认刷新令牌所有权，避免两个客户端同时刷新同一令牌。导入脚本默认隔离配置，见 `scripts/copy-config-isolated.ps1`。

## 当前能力

- 统一逻辑模型与来源：GPT 按本机原生目录优先级显示，再排列第三方模型；切换 API 来源保持模型身份和顺序。模型映射兼容不同上游命名。
- 来源池与右键菜单区分订阅类型、登录身份、工作区和席位；订阅日期及查询时间来自缓存，授权过期不等于订阅到期。
- 保留原生思考等级。Astra/Sol 空目录按核实过的 Codex 内置能力回退，Luna 不变；精确模型快照位于 `reference/native-reasoning.json`，新鲜非空目录优先。
- 同模型自动切换需逐来源勾选；明确限流、流开始前切换，不降级。HTTP/WS 账号绑定和原生 Responses 压缩有合成上游覆盖。
- 自动来源支持精确模型映射的 Responses 和 Chat Completions API；不再仅因 Chat 协议拒绝勾选。不支持的旧来源可取消勾选，不放开跨模型降级。
- Gemini 思考预设合并为模型内等级，绑定所选来源的实际变体。
- 模型/余额探针、API 模型开关、来源设置及自动目录刷新。
- 订阅用量按可编辑等效倍率累计，两个满额 1× 席位显示约 200%；五小时受限池与周池独立。API 显示当前逻辑模型可用的真实余额，按币种合并并去重。
- 静默桌宠、右键菜单、最新主对话换装、捏一捏气泡、长按切换简约/详细、星空额度条和平滑动画。音效可从本地导入。
- 每次捏桌宠主动刷新额度；进行中的重复动作合并，不排队刷屏。API 仅查询余额，不触发模型目录探针；失败保留上次有效余额，不误标账号过期。

## 验证与边界

`npm run test:selfuse` 检查隔离与界面行为，`npm run test:isolation-native` 检查独立路径，`tests/compat-harness` 检查协议兼容。`scripts/smoke-desktop.ps1 -RoutingTests` 使用真正 Windows 程序、临时目录和合成上游；`scripts/test-catalog-client.py` 用本机 Codex 验证模型目录解析，不发推理请求。

已验证范围以 [版本记录](VERSION_HISTORY.md) 为准。合成测试不等于全部真实提供方均可用。Chat Completions 不支持原生压缩时返回 501，跨协议压缩尚未实现；Google GPT OSS 工具模板错误及部分 Gemini 版本超时仍取决于上游，不能视作全部解决。

## 构建与隐私

标准环境安装 Node、Rust 和 Tauri 对应平台依赖后运行 `npm ci`、`npm run tauri build -- --no-bundle`。`scripts/build-selfuse.ps1` 用于工作区已有的便携 Windows GNU 工具链，并重映射编译路径；工具链本身不入库。发布包用 `scripts/package-preview.ps1` 的明确文件清单生成，不包含账号、密钥、聊天、日志或备份。发布前还需扫描源码和可执行文件中的个人路径与凭据。

## 许可

上游代码遵循 [MIT](LICENSE)。本包的角色动画来自 fywdy/chatgpt-chibi-pets，遵循 **CC BY-NC 4.0，仅限非商业使用**，不属于代码 MIT 许可。完整来源和声明见 [素材说明](public/pets/NOTICE.md)。其他鲸鱼娘素材/音效未随包分发，可由用户自行导入。
