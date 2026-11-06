# 发布与历史构建归档

当前使用版本：[preview 18](https://github.com/YuYiChyan/switcher-pet/releases/tag/preview-2026.11.06.18)。

现存构建共 17 个 Windows 修订包及 1 个 macOS arm64 包。preview 1–13 的脱敏历史包集中在一个历史 Release；preview 14–18 保留各自 Release。源码、版本说明和校验清单存放于 Git。

## 早期包的来源与处理

早期 12 个 Windows 包保留了编译目录、链接符号和应用元数据中的本机构建身份。公开归档副本执行了等长脱敏：替换只读数据段中的编译路径、COFF 字符串表中的目录名称、旧应用标识中的个人名字，以及 CompanyName 资源。随后重算 PE 校验和、包内 SHA256 清单及 ZIP 校验文件。

代码段与节区布局保持逐字节一致。旧应用标识的匿名化可能让 WebView 使用新的数据目录，因此这些副本定位为历史研究归档，日常运行请选择当前发布版。此次归档完成静态 PE、资源清单、内容哈希与隐私检查；历史版本的启动和真实账号行为沿用各自原有验证状态。

原始未脱敏包不随公开归档发布；公开副本经匿名化，不能视作原始包的逐字节备份。归档副本带有 `ARCHIVE-PROVENANCE.json`，记录原始/归档哈希、处理类型与源码来源。实现脚本为 [`archive-historical-preview.py`](../scripts/archive-historical-preview.py)。

preview 1–13 的源码在 preview 14 才合并提交；仓库保留这个真实的合并节点，而不是为历史包补造精确源码标签。不得将未来历史 Release 的索引提交误认为各早期二进制对应的源码快照。preview 7 是开发节点，preview 11 是中间构建。

## 全部版本

| 修订 | 主要变化 | 发布入口 |
| --- | --- | --- |
| preview 1 | 独立标识、数据路径隔离与 Windows 便携启动 | [下载](https://github.com/YuYiChyan/switcher-pet/releases/tag/archive-previews-1-13) |
| preview 2 | 模型路由、协议兼容、账号和子任务绑定 | [下载](https://github.com/YuYiChyan/switcher-pet/releases/tag/archive-previews-1-13) |
| preview 3 | 模型开关、额度池、探针与桌宠入口 | [下载](https://github.com/YuYiChyan/switcher-pet/releases/tag/archive-previews-1-13) |
| preview 4 | 导入信息恢复、探针和静默启动 | [下载](https://github.com/YuYiChyan/switcher-pet/releases/tag/archive-previews-1-13) |
| preview 5 | 捏一捏、音效与桌面交互修正 | [下载](https://github.com/YuYiChyan/switcher-pet/releases/tag/archive-previews-1-13) |
| preview 6 | 简洁余额组件、随机气泡与 Google 工具转换 | [下载](https://github.com/YuYiChyan/switcher-pet/releases/tag/archive-previews-1-13) |
| preview 7 | 默认代理端口统一为 18080 | 开发记录 |
| preview 8 | 长按切换视图、星空额度条和统一账号来源 | [下载](https://github.com/YuYiChyan/switcher-pet/releases/tag/archive-previews-1-13) |
| preview 9 | 五小时受限池与模型目录刷新 | [下载](https://github.com/YuYiChyan/switcher-pet/releases/tag/archive-previews-1-13) |
| preview 10 | 原生/API 逻辑模型合并及统一路由选择器 | [下载](https://github.com/YuYiChyan/switcher-pet/releases/tag/archive-previews-1-13) |
| preview 11 | 目录解析与额度显示修正的中间构建 | [下载](https://github.com/YuYiChyan/switcher-pet/releases/tag/archive-previews-1-13) |
| preview 12 | Gemini 思考归并、Business 去重和大数额度 | [下载](https://github.com/YuYiChyan/switcher-pet/releases/tag/archive-previews-1-13) |
| preview 13 | API 实际余额与币种显示 | [下载](https://github.com/YuYiChyan/switcher-pet/releases/tag/archive-previews-1-13) |
| preview 14 | GPT 原生排序、模型目录合并与发布脱敏 | [下载](https://github.com/YuYiChyan/switcher-pet/releases/tag/preview-2026.11.06.14) |
| preview 15 | Astra/Sol 思考等级和订阅/工作区身份 | [下载](https://github.com/YuYiChyan/switcher-pet/releases/tag/preview-2026.11.06.15) |
| preview 16 | Google 重新授权绑定与工具枚举适配 | [下载](https://github.com/YuYiChyan/switcher-pet/releases/tag/preview-2026.11.06.16) |
| preview 17 | Claude 形象比例修正 | [下载](https://github.com/YuYiChyan/switcher-pet/releases/tag/preview-2026.11.06.17) |
| preview 18 | Chat API 同模型切换、捏桌宠刷新和 macOS 菜单栏 | [下载](https://github.com/YuYiChyan/switcher-pet/releases/tag/preview-2026.11.06.18) |

机器可读的各包 SHA256、原始哈希、源码提交与归档状态见 [`manifest.json`](manifest.json)。每个已发布 ZIP 对应的 `.sha256` 文件随 Release 提供。
