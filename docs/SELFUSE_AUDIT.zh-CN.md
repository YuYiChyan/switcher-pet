# Codex Switcher 自用版源码审查

审查日期：2026年11月6日。上游：[VallierDev/codex-switcher](https://github.com/VallierDev/codex-switcher)，提交 `ddbee503e50bb369e3c68574e9c65b78e9d50649`，包版本0.7.21，MIT代码许可。

本机可执行文件版本同为0.7.21，数据结构也相符；没有可重现构建证据证明本机二进制与该提交逐字节对应。本报告将源码缺口与本机已复现故障分开。以下发现及行号对应上游基线；后续实现进度以仓库根目录 README.selfuse.md 为准。

## 1 已确认的源码问题

### P1 原生中转专用压缩分支不可达

位置：`src-tauri/src/proxy.rs:1931-1935,1994-2004`。

`handle_named_relay_response` 首先允许 `/responses` 和 `/responses/compact`。进入原生 Responses 分支后，第二次检查却只允许以 `/responses` 结尾，导致 compact 在到达 `forward_native_compact` 前被400拒绝。专用转发函数已存在，但该入口到不了。

这可以解释该入口的压缩失败；不代表全部非 CDX 压缩失败都来自这一个错误。应移除重复矛盾校验或统一路径判定，并添加真正经过 HTTP handler 的 mock 回归测试，检查 URL、凭据归属、请求历史和返回内容。

### P1 Chat Completions 中转没有上下文压缩实现

位置：`src-tauri/src/proxy.rs:8729-8730,9163-9175`。

遇到 compact 直接返回429，错误码为 `compaction_not_supported`。这不是压缩成功，也不是上游余额耗尽。该策略可能让客户端绕过本次压缩，但无法保证长对话最终不越界。

原生 Google handler 同样只接收普通 `/responses`，见 `proxy.rs:1211-1216`。应建立原生／适配／不支持三类压缩能力，永久不支持不能无限短重试。

### P1 WebSocket 会话硬绑定没有覆盖全部账号类型

位置：`src-tauri/src/proxy.rs:3392-3424`。

模型提示分流先于硬路由；硬路由分支明确只处理 Chat Completions relay，原生 Responses relay 注释写明未实现 per-request 覆盖、退回 current。订阅账号也有未覆盖限制。HTTP 路径中命名 relay／Google 模型入口也先于后续硬路由解析，见 `3459-3509`。

因此“为会话设定账号”不是所有传输方式下的一致保证。子任务独立会话也不会自然继承父绑定。修复目标应是一个不可变 RouteDecision 贯穿 HTTP、WebSocket、compact 和重连，而不是在请求间修改共享 current。

### P1 工具返回的图片可能在转译时丢失

位置：`src-tauri/src/relay_translate.rs:650-703`，`src-tauri/src/antigravity/translate.rs:162-176`。

共享工具输出归一化主要累积文本，数组分支只处理 text/input_text；Google 的 functionResponse 又包装为纯文本 output。客户端返回截图等内容不能据此认为已完整保留。

应使用有类型的内容块并补全跨协议映射；本机执行工具本身成功不代表模型能看见工具返回的图片。

### P1 不认识的历史项会静默丢弃

位置：`src-tauri/src/relay_translate.rs:354-370`。

输入项类型 match 的默认分支为空。compaction 等未处理类型可能被无声跳过。对不透明状态不应假装成功转译；应拒绝不支持的历史，或使用有验证依据的上下文桥，并提示用户。

### P1 工具 Schema 清洗并非无损

位置：`src-tauri/src/antigravity/translate.rs:311-408`。

联合类型只取部分分支，若干引用和组合关键字被移除。复杂工具声明可能改变语义。这与“Gemini不能原生调用许多工具”的反馈方向一致，但未证明每个失败工具都由此引起。需要保留约束、预解析引用和逐工具兼容报告。

### P0 并行启动 fork 会碰到原版数据和登录状态

位置：`account.rs:777-796`、`session_routes.rs:57-62`、`lib.rs:6181-6185` 及多处用户目录调用；`tauri.conf.json`。

账号路径隔离只在 debug 编译条件下存在；会话路由、日志、技能、Codex 配置等仍有直接用户目录路径。产品标识、深链和默认端口也沿用原版。不能现在直接运行克隆应用。

同一 OAuth refresh token 被两份程序刷新存在所有权风险。迁移不能只是复制 accounts.json 后同时启动。

### P1 Windows 凭据落盘保护不足

位置：`src-tauri/src/account.rs:748-769`。

文件和目录的显式权限加固只在 Unix 分支实施，非 Unix 分支直接成功返回。现有账号文件包含凭据字段。没有证据称本机文件已对其他用户开放，但新版本必须验证 Windows ACL，并将凭据从一般 JSON 设置迁到系统保护存储。

## 2 需要真实请求证据才能定论的问题

### Google 平台全部非 Gemini 报错

源码已经有 Claude 和 GPT OSS 支持，不应当按“根本没实现”处理。存在 Claude thinking 与 forced tool_choice 的显式互斥，模型能力大量按名称推断，GPT OSS 的默认思考值与允许集合还需单独核对。

其中一个确定的条件缺陷：`antigravity/models.rs:53-73` 对 GPT OSS 只允许 medium，但未带 `-medium` 后缀的模型会获得 high 默认值；`translate.rs:96-104` 又先校验允许列表、再进入 GPT OSS 分支。客户端若发送该默认 high，会在到达上游前被拒绝。这能解释此类请求，不足以解释全部 Claude 失败。

`docs/GOOGLE_CPA_PARITY_AUDIT_2026-09-03.md` 记载上游曾通过部分 Gemini／Claude 工具往返，同时明确列出多模态、签名、增量会话等缺口。这是上游记录，不能当作本机当前回归结果。

下一步收集不含正文的错误指纹和平台模型目录快照，逐一核对请求实际落在原生 Antigravity 还是 relay。不要因为一个400轮询所有账号；也不要未经验证把所有非 Gemini 假装成 Gemini。

### 子智能体能否自由指定第三方

同时受到 Switcher 路由和 Codex 客户端／服务端执行架构影响。代理收到的每个子请求能否识别父会话、客户端是否允许该模型、请求是否经过本机，均需在独立客户端验证。没有这些条件，做一个子智能体模型下拉框并不能兑现功能。

## 3 本轮实际观察和未执行事项

- 本机原程序为0.7.21；初始可执行文件SHA256：`FD8CBF4AE6FA5BD85060E4FE029177926A4A2F6B25378B86958D9C08F192953B`。
- 只读识别11个账号记录：7个legacy、3个relay、1个antigravity_oauth。legacy不自动等价于7个Plus账号。
- 只读查看字段结构和必要的非凭据路由字段；未导出或复制账号令牌，未写账号库，未刷新登录。
- 本轮一个现有 web 工具调用实际返回400和 `Selected relay model requires the Responses API`。该文本与源码拦截文本相同，但没有请求路径和调用链证据，不能据此断言这次错误就是 compact 缺陷；也可能是其他非 Responses 路径。
- 没有启动、停止、重装或重启原版；没有改 Codex 配置、Windows 注册表、自启动或当前端口。
- 本地 fork 分支为 `selfuse/pet-router`，远端名 `upstream`。未创建用户 GitHub 账号下的远程 fork，未推送。缺少可用的 GitHub 登录管理工具与明确目标账号，不猜测目的地。
- 未安装新构建工具链，未编译Rust/Tauri，未用真实密钥打探针。上游测试数字不能写成本轮测试结果。

## 4 优先修复清单

1. 隔离全部文件、进程、端口、更新和令牌刷新所有权，允许安全启动测试实例。
2. 为原生 compact 分支建立失败用例；修复双重校验，并测试查询参数、无效路径和凭据隔离。
3. 统一路由优先级及不可变会话绑定，验证两主对话与子任务同时使用不同来源。
4. 保留工具图片／文件结果、未知历史报错、复杂 Schema 和 thought signature 配对。
5. 分平台／模型能力探针；非原生压缩先做客户端可接受性验证，再实现可携带摘要。
6. 在上述基础上做模型菜单、用量账本和桌宠，避免界面承诺超过实际能力。

配套产品设计见同目录 `SELFUSE_PRODUCT_DESIGN.zh-CN.md`；素材取证清单见 `SELFUSE_ASSETS.json`。审查后第一步已增加独立路径、产品身份、默认端口、实例锁和危险原版操作拦截；协议与路由问题尚未修复，不将静态证据或隔离模块测试称为上线修复。
