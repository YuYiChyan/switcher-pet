# Switcher Pet · 自用开源版

**把模型切换、账号池与额度监控，放进一只桌宠。**

Switcher Pet 基于 [Codex Switcher](https://github.com/VallierDev/codex-switcher)，为 Codex 的多账号和第三方模型使用场景增加统一路由、同模型自动换源、额度池和拟人桌宠入口。日常在桌面查看用量、右键选择模型与来源；需要配置时再打开设置中心。

[下载 preview 18](https://github.com/YuYiChyan/switcher-pet/releases/tag/preview-2026.11.06.18) · [全部版本](https://github.com/YuYiChyan/switcher-pet/releases) · [版本记录](VERSION_HISTORY.md) · [macOS 使用说明](README.macOS.md)

<p align="center">
  <img src="public/pets/deepseek.webp" width="140" alt="DeepSeek 桌宠">
  <img src="public/pets/gpt.webp" width="140" alt="GPT 桌宠">
  <img src="public/pets/claude.webp" width="140" alt="Claude 桌宠">
  <img src="public/pets/gemini.webp" width="140" alt="Gemini 桌宠">
</p>

## 平台与下载

| 平台 | 发布包 | 启动方式 |
| --- | --- | --- |
| Windows x64 | `SwitcherPet-preview-20261106-18.zip` | 解压，打开 `Open-preview.cmd` |
| macOS Apple Silicon | `SwitcherPet-macOS-arm64-preview18.zip` | 解开内层 `.app.zip`，将应用放入 Applications |

当前发布版为 **preview 18**，基础应用版本为 `0.7.21`。下载时按 preview 修订号区分版本。Windows 使用 WebView2 Runtime；macOS 面向 Apple Silicon，使用 ad-hoc 签名，首次打开按系统提示为这个应用单独放行。

## 主要功能

### 一个模型选择器，统一订阅与 API 来源

- GPT 按本机原生模型目录的优先级排列，第三方模型接在同一套选择器中。
- 模型的二级菜单选择路由来源：订阅账号、OpenAI API、第三方 API 或 Google 来源。
- 逻辑模型与上游模型名分别管理。API 别名映射到同一逻辑模型后，可以与订阅来源一起选择。
- 来源卡片展示订阅类型、登录身份、工作区与席位，便于区分个人账号和工作区身份。
- API 模型支持逐项启停；模型探针与目录刷新把新发现的模型带回选择器。

### 自动切换保持同一个模型

在来源配置中勾选允许自动切入的来源。遇到明确限流时，代理可在输出流开始前选择同模型的可用候选，保留当前逻辑模型。

自动来源接受精确映射的 Responses 和 Chat Completions 接口；手工指定来源、会话绑定和自动候选分别处理。HTTP、WebSocket 与原生压缩入口共用模型路由规则，子任务可以通过会话绑定指定来源。

### 原生思考等级与 Google 适配

- 保留 GPT 原生思考等级，修复 Astra / Sol 目录为空时思考条消失的问题。
- Gemini 的思考预设合并到模型内的等级选项，按所选来源映射到实际变体。
- Google 账号重新授权后修复旧来源绑定，改善失效引用导致的请求失败。
- 补齐工具参数的布尔、数字枚举处理，以及工具结果中的图片和结构化内容转译。

### 看得出账号池倍率的额度显示

额度按 **可编辑的等效倍率**累加，而不是把整池压成 100%。例如两个满额 1× 账号显示约 `200%`，更大的池可以继续显示 `500%`、`1000%`。`≈` 表示倍率估算，实际可用量以各来源返回的数据为准。

- 五小时受限池与周额度分别统计；存在五小时窗口时优先显示短窗口，全池确认只有周限制时显示周额度。
- 支持总池和独立账号视图，混合套餐的权重可在设置中编辑。
- API 展示当前逻辑模型可调用来源的实际余额金额与币种；共享余额去重，不同币种分别计算。
- 配置余额基准后，API 也可显示额度百分比；金额仍保留为直接读数。
- 探针失败保留上次有效记录，并区分未知、缓存与真实耗尽状态。

### 桌宠就是日常入口

内置 DeepSeek、GPT、Claude、Gemini、Grok、Kimi、Qwen、GLM 八套社区同人形象。可固定角色，也可随最新活跃主对话的模型家族自动换装。

| 操作 | 效果 |
| --- | --- |
| 右键桌宠 | 选择模型、来源、形象，打开设置 |
| 捏一捏 | 形变与回弹、随机对话气泡，同时主动刷新额度 |
| 长按 | 切换简约 / 详细额度视图 |
| 展开额度条 | 查看大数百分比、五小时与周额度、API 金额 |
| 声音设置 | 导入 MP3 / WAV，试听、调节音量或静音 |

额度条使用星空配色，展开与收回带平滑过渡。简约模式将额度条放在模型名旁，详细模式展开池信息。应用默认静默启动，先显示桌宠与托盘，设置通过右键打开。

自动换装结合本机主对话模型元信息与代理请求事件，以最新主回合为依据；明确标识的子任务和探针不会抢占形象。它反映最近活动的主对话，而非操作系统前台窗口焦点。

### macOS 内置菜单栏监控

菜单栏可切换总池、当前模型池、当前账号，并选择五小时优先或周额度。订阅倍率以大数百分比显示，API 以金额显示，和桌宠共用同一份额度缓存。

捏桌宠或点击“立即刷新额度”都会触发刷新。重叠操作合并，每次最多并行查询三个来源；API 刷新仅查询余额，模型扫描由目录探针负责。

## 开始使用

1. 从 Releases 下载对应平台的 ZIP，核对同页 `.sha256` 校验文件后解压。
2. 启动应用，右键桌宠进入设置，添加订阅账号或 API 来源。
3. 刷新模型目录，核对逻辑模型与上游模型名；选择需要启用的 API 模型。
4. 在每个模型的来源配置中选择默认来源，并勾选允许自动切换的候选。
5. 配置账号池倍率、余额基准及桌宠形象；按需导入本地音效。

代理默认端口为 **18080**。已有服务占用时，先在设置中选择另一端口，再让客户端连接对应地址。迁移 OAuth 账号时，为刷新令牌确定唯一的管理实例，避免两个程序同时刷新同一账号。

Windows 便携版配置保存在 `%USERPROFILE%\.switcher-pet\preview\profile`；macOS 数据位于 `~/.switcher-pet/`。升级时保留配置目录，替换应用包即可。账号数据应单独备份，发布包与配置分别管理。

## 协议能力与使用边界

| 功能 | 当前实现 |
| --- | --- |
| 原生 Responses | 请求转发、流式输出、指定来源、原生 compact |
| Chat Completions API | Responses 请求转译、精确同模型自动候选、工具内容适配 |
| Google 来源 | 模型映射、思考变体、工具参数转译、会话绑定与同模型候选 |
| 跨协议 compact | 待实现；不支持的 Chat 接口返回 501，便于与限流区分 |
| 余额探测 | 按提供方适配器查询；未知结果与实际零余额分别显示 |

Google GPT OSS 的工具模板错误及部分 Gemini 版本的超时仍可能来自上游。遇到问题请附带模型名、协议、应用修订号、脱敏错误码和复现步骤。授权有效期与订阅有效期分别判断。

## 从源码构建

技术栈为 **Tauri 2 + Rust + React 19 + TypeScript + Vite**。准备 Node.js 22、Rust stable，以及所选平台的 Tauri 编译依赖。

```sh
git clone https://github.com/YuYiChyan/switcher-pet.git
cd switcher-pet
npm ci
npm run tauri build -- --no-bundle
```

Apple Silicon 应用包：

```sh
rustup target add aarch64-apple-darwin
APPLE_SIGNING_IDENTITY="-" npm run tauri build -- --target aarch64-apple-darwin --bundles app -- --locked
```

Windows 可使用标准 Rust / Visual Studio 构建环境。本项目另附 `scripts/build-selfuse.ps1`，适用于预先准备好 `.tooling` 便携 GNU 工具链的开发环境；打包脚本为 `scripts/package-preview.ps1`。发布构建应重映射编译路径，并检查可执行文件中的调试符号与个人标识。

macOS 自动构建入口：[Apple Silicon workflow](.github/workflows/mac-preview.yml)。工作流执行测试、构建、架构检查、签名检查及隔离启动，并生成可下载的应用包。

### 测试

```sh
node --test scripts/*.test.mjs
cargo test --locked --manifest-path tests/compat-harness/Cargo.toml
cargo test --locked --manifest-path tests/isolation-harness/Cargo.toml
```

Windows 桌面回归入口为 `scripts/smoke-desktop.ps1 -RoutingTests`，使用临时目录与合成上游。preview 18 的发布记录包含 45 项 Node 测试、78 项兼容测试、8 项隔离测试及 36 组 Windows 桌面检查。真实提供方的行为还受账号权限与上游服务状态影响。

### 目录结构

| 路径 | 用途 |
| --- | --- |
| `src/` | 桌宠、额度组件、模型选择与设置界面 |
| `src-tauri/src/` | 本地代理、路由、账号、余额探针与托盘 |
| `public/pets/` | 角色动画、来源清单与素材许可 |
| `tests/` | 协议兼容和路径隔离测试 |
| `scripts/` | 构建、打包、迁移与回归工具 |
| `reference/` | 原生模型能力参考快照 |
| `release-history/` | 各版归档索引、校验值与来源说明 |

## 版本沿革

从独立运行基础，到统一模型路由、桌宠与额度池，再到 Google 适配和 macOS 菜单栏，完整变化见 [VERSION_HISTORY.md](VERSION_HISTORY.md)。

preview 1–13 的脱敏历史构建见 [早期版本归档](https://github.com/YuYiChyan/switcher-pet/releases/tag/archive-previews-1-13)；preview 14–18 分别保留 GitHub 发布记录。preview 7 以开发记录保存，preview 11 标记为中间构建。早期源码在 preview 14 合并入库，历史包的校验值、脱敏过程与源码对应关系见 [归档说明](release-history/README.md)。日常使用选择 preview 18。

## 贡献与许可

欢迎提交模型兼容报告、余额适配器、界面改进和回归测试。提交前请移除账号、令牌、聊天内容、个人目录和日志中的身份信息；测试优先使用合成数据。

代码继承 [VallierDev/codex-switcher](https://github.com/VallierDev/codex-switcher)，遵循 [MIT](LICENSE)。感谢上游的多账号管理与代理基础。

角色动画来自 **fywdy/chatgpt-chibi-pets**，采用 **CC BY-NC 4.0**，适用于非商业使用，须保留署名和许可。代码与美术分别遵循各自许可，详见 [素材声明](public/pets/NOTICE.md) 和 [素材清单](public/pets/manifest.json)。角色属于社区同人创作；各模型品牌名称用于说明对应模型家族。本地导入的音效由使用者按其授权范围使用。
