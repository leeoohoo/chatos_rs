# 插件使用逐项验收

日期：2026-10-13。范围：macOS、shared Host及仓库六类插件；不实施Windows native/UI。

## 安全与证据边界

- 使用隔离临时目录、虚构数据和本地测试页面，不重试旧音乐任务、不操作真实账务、不修改财务项目。
- 不覆盖已安装插件，不把源码测试通过冒充已安装版本验收。
- App实机复测只使用 `/Applications/ChatOS.app`：先核对进程完整路径、正常退出旧实例、备份及重装，再按绝对路径启动。不得按应用名称定位或同时启动仓库构建版；`getApp`可能隐式启动另一份同名应用。
- 分别记录源码/单元测试、真实MCP协议、native接入、模型使用、UI/产物证据；未执行部分保持未完成。
- 浏览器/Computer Use的实际界面动作遵守当前工具限制；必要的系统权限交给用户确认。

## 任务清单

- [x] 1. 通用接入：能力发现、描述、Skill激活/引用、参数校验、版本冻结、错误回传与审批（自动化层）。
- [ ] 2. 浏览器：完整源码测试、Skill门控审计、native安装版测试契约核对、可恢复失败与本地页面验证。
- [x] 3. 文档：Word/Excel/PPT/PDF创建或编辑、检查、渲染、产物路径与实际MCP测试（隔离虚构数据）。
- [ ] 4. Computer Use：源码测试、Skill门控、权限及实际操作覆盖边界。
- [x] 5. 图表设计：源码/MCP测试、Skill门控、图表产物与交付（自动化层）。
- [x] 6. 方案设计：源码/MCP测试、Skill门控、上下文隔离与方案交付（自动化层）。
- [x] 7. 网页设计：源码/MCP测试、Skill门控、场景生成、导出及校验（自动化层）。
- [x] 8. 对本轮发现的问题逐项修复、增加回归/CI检查，并重新验证受影响链路。

## 首轮源码验证记录（发布候选构建前）

| 范围 | 本轮证据 | 尚未验收 |
| --- | --- | --- |
| Browser 0.1.15源码 | 37项通过；6个Skill/39个工具门控通过；真实安装版及仓库构建版均经native stdio initialize/tools-list及schema校验 | Chrome页面操作；1项需要Chrome的Rust测试ignored |
| Document 0.1.8源码 | 11项真实MCP测试通过，Office四格式、PDF操作、检查、渲染及产物；15个工具/5个Skill门控通过；typecheck通过 | 所有模板、长文档及真实模型使用覆盖 |
| Computer Use 0.8.18源码 | 16项Swift测试通过；11个工具/5个Skill门控通过 | 用户桌面点击、键盘输入及系统授权实机操作 |
| Diagram 0.4.5源码 | 57项通过，含真实MCP、校验、隔离、幂等；18个工具/7个Skill门控通过 | 全部模板视觉及真实模型调用 |
| Solution 0.1.9源码 | 16项通过，含真实MCP保存、图校验、finalize/产物及源目录不写；10个工具/5个Skill门控通过 | 完整用户项目的模型业务场景 |
| Web Design 3.0.30源码 | 322项通过，含MCP、Scene生成/修复；18个工具/9个Skill门控通过 | 全部场景视觉及真实模型端到端 |

完整 `native-platform` 本机CI入口通过；真实Host启动及仓库Browser catalog测试不skip。6项可选环境测试仍skip（含安装版Browser操作）；安装版和仓库版的只读catalog测试额外启用环境后均通过。政策测试8项通过；本轮5个修改Skill的validator通过。这不代表所有真实模型场景通过。

日志位于 `/tmp/chatos-plugin-{browser,document,computer,diagram,solution,web}-test.log`、`/tmp/chatos-plugin-*-audit.log`、`/tmp/chatos-plugin-native-platform-test.log`、`/tmp/chatos-plugin-browser-native-catalog-test.log`、`/tmp/chatos-plugin-policy-test.log`。临时日志不是版本库交付文件。

## 修复及回归

1. 四类router误指导 `skill_skill_*`，而实际Host只有 `agent_skill_*`。统一按真实catalog的 `skill_ref` 激活、按 `relative_path` 读取，不让模型构造内部evidence。Browser npm副本同步。
2. 旧安装版/冻结Skill原文保持不变；activation响应额外提供 `host_skill_protocol`，映射旧名称至实际Host名称，不创建别名工具，不改变冻结hash、审批及权限。真实stdio fixture覆盖原文保留、映射和业务调用。
3. 插件Skill打包审计拒绝三个过期控制工具名，并加入负例；六插件现有CI/prepack审计因此阻止新版本再次携带旧指引。
4. Browser原生可选业务测试改用原安装记录（含逐文件hash），移除过期参数，目标改为隔离data页面。新增仓库临时包catalog测试；native CI先构建Browser并强制校验真实schema，缺依赖不能静默skip。不注册用户Chrome bridge、不调用页面业务工具。
5. PPT textbox/set_text新增可选color/size，支持明确设置浅色文字；旧未传样式的调用不变。真实MCP生成后逐text run核对颜色/字号，再检查渲染图标题及正文区域的浅色像素；非法颜色不得发布产物。深色背景浅色文字已目视确认。SBOM按正式生成入口更新并通过check。

## 源码与当前安装版

安装版Solution为0.1.8、Computer Use为0.8.17、Diagram为0.4.4、Web Design为3.0.25；Browser/Document虽与源码版本号相同，也不能推断已包含本轮补丁。本轮未发布或覆盖插件。PPT新字段和router修改要通过正常新Release发布/升级才会进入安装版schema；不能复制源码到受完整性保护的安装目录。

App已按用户要求备份、替换并重启 `/Applications/ChatOS.app`（debug构建 `2026-10-13T07:49:16Z`）。旧安装版保留在 `/Applications/.chatos-reinstall-backup.JjnObC/ChatOS.app`。进程核对只有安装路径的一个App及一个Host；联系人、项目与历史消息加载正常；Run状态数量无增量，财务9文件hash不变。此次误用应用名定位曾开启第二实例，已退出并修正实机操作规则。

## 未完成与质量门禁

- Browser真实页面和Computer Use真实桌面操作尚未完成，任务2/4保持未勾选。
- 真实模型逐插件业务验收、50场景全量评测、全部设计视觉验收仍未执行，不能由单元/MCP测试替代。
- 当前工作区 `check_source_size_policy.py --scope owned` 仍报告13个超限文件（含Task/回写及native provider测试等）；完整仓库质量门禁不能宣称通过。未通过加allowlist掩盖，后续应拆分文件并复测。

## 正式Release候选与发布状态（2026-10-13追加）

用户已明确授权发布到自有插件平台并升级本机；未授权发布到npm公共仓库。以下候选包已正式发布，Release ID见下表；不等同于本机升级或模型/UI验收完成。

| 插件 | 候选版本 | 当前安装版记录 | 候选包SHA256 |
| --- | --- | --- | --- |
| Browser | 0.1.16 | 0.1.15 | `87de906cfe87cfd002de222aa8f313e4e7850d2823e496e67f9fe18ef42f4ba9` |
| Document | 0.1.9 | 0.1.8 | `10acd62579689f18e96e0cd0b386b300ce0fe249e15f0d465aab8d741ee674e5` |
| Computer Use | 0.8.19 | 0.8.17 | `ac7d3ed04073086aca68ccbd4d079e049359b9de92997c164e90242b5c57b30b` |
| Diagram | 0.4.6 | 0.4.4 | `368e81d4d70fe59380807d78ffdf88cdea8e8b168ca381f31cd9c4a0419cd717` |
| Solution | 0.1.9 | 0.1.8 | `8a7eade283c39e8e4e92ffe98d2c480a1e022a8fff2070a32b2e26842daee8b3` |
| Web Design | 3.0.30 | 3.0.25 | `cf0a98c1bd7658aef412e74a5920674ad5d0a8b7b431756216df5104f4574200` |

- 候选tgz及解包目录：`/tmp/chatos-plugin-releases-Rj0N9D`。此为本机临时产物，不能当作永久发布地址。
- 六类源码Skill gate和六个解包候选的真实MCP initialize/tools-list审计通过，新增核对 `serverInfo.version` 与manifest版本一致。版本不一致负例已加入政策测试。
- 审计子进程禁用Browser extension注册并移除继承bridge endpoint，Computer Use managed app根目录隔离，避免仅检查catalog就触碰用户应用安装。
- Browser重新通过cargo fmt检查、37项Rust测试（1项Chrome业务测试仍ignored）；npm测试从6项增至9项。两份Chrome验收脚本通过共享契约移除过期mode/session ID参数，单次打开后核对实际mode，失败/降级关闭，不循环创建会话。新增无UI模拟回归覆盖成功、降级、业务失败及打开失败。脚本真实Chrome操作未运行。
- Document真实包解包后完成15工具MCP检查及OfficeCLI创建DOCX；CU打包/16项Swift测试、Diagram57项、Solution16项、Web322项通过。日志见 `/tmp/chatos-release-*-{build,test,pack}.log`，候选catalog见 `/tmp/chatos-release-packed-audit.log`。
- CU候选仅包含arm64二进制，manifest改为 `macos-arm64`；打包入口使用lipo检查声明与实际架构一致，不冒充Intel包或Windows实现。该候选重新生成后的SHA256以上表为准，不使用旧候选hash。
- Skill gate/startup政策测试合计9项通过。财务项目9文件hash再次核对不变。现有进程仍为 `/Applications/ChatOS.app/Contents/MacOS/ChatOSSwift` 与其Host，未另启客户端。

初始发布阻塞（历史记录）：Chrome管理端页面状态不可靠且回到登录页。不提取浏览器cookie/token，不直接覆盖插件目录。该初始状态已被下面的实际发布进度取代；尚未升级的本机安装版不能宣称包含候选修复。

### 平台正式发布进度

已通过用户登录的管理端正式上传、分析、发布，并在Release列表核对版本、stable通道、有效状态、签名密钥和候选SHA256：

| 插件 | 版本 | Release ID | 发布时间（北京时间） |
| --- | --- | --- | --- |
| Browser | 0.1.16 | `17d49990-1625-459e-8c3b-d58a22c7c04b` | 2026-10-13 17:08:46 |
| Document | 0.1.9 | `60f5755f-78cd-43ce-b82d-28368a66acd2` | 2026-10-13 17:21:26 |
| Computer Use | 0.8.19 | `483266ae-000b-4572-9614-52cf61f55e74` | 2026-10-13 17:31:51 |
| Diagram | 0.4.6 | `6b9d1057-3b54-4de0-9e27-bb3224702927` | 2026-10-13 17:43:51 |
| Solution | 0.1.9 | `6896ea59-1650-415c-95f1-1840c99a8111` | 2026-10-13 17:53:26 |
| Web Design | 3.0.30 | `851f20a1-3d33-4e41-bc1b-3487b1a00eb7` | 2026-10-13 17:53:27 |

六类最新stable Release已通过正式API再次核对；旧Release仍有效，保留现有publisher/license/visibility/featured，未撤销旧版或扩展权限。本机升级及实际模型/UI验收尚未完成。

2026-10-14追加：Solution的T004跨需求图/计数/导航窄修已发布为0.1.10，Release `9c37e31c-02e3-4340-bf67-8feed546a140`，SHA256 `df4a83ee93a6e9f26692012a6b4a18bff698ca20ecb42a923bbcbb34f59ae554`，北京时间09:04:44发布。已回读确认latest/stable/有效，0.1.9保留；26项回归在Node24与Node20.18.1通过，解包MCP门控10工具/5 Skills通过。用户用原账号重新登录后，本机通过正式更新入口于09:16:45安装0.1.10，显示校验安装成功，Release/hash和29个逐文件完整性记录全部一致。没有使用管理端账号替换用户、改写安装记录或启动第二份App；T004安装版交互验收仍待完成。详细范围及后续未实现事项见[Solution优化方案](solution-studio-optimization-plan.md)。

前四类经管理端UI发布。随后Mac锁屏，在用户明确提供本次平台登录账号后，使用正式程序登录接口取得仅在内存使用的认证令牌，通过同一 analyze/publish API发布后两类；不读取浏览器cookie/token。发布前对比权限集合与旧Release一致，已有同版本时仅核对hash，不覆盖不可变Release。认证令牌已从执行会话变量清除；账号密码不写入文档或本地产物。

管理端 `admin_console/src/shared/auth/tokenStore.ts` 当前仅在内存保存管理员登录凭证，主动清除持久化token；刷新会退出登录。此次核对结果的刷新曾导致重新登录，后续采用站内导航，不再刷新。此处不修改或降低认证保护。

登录后的执行顺序：上传tgz分析 → 核对插件标识、版本、架构、权限与现有publisher/license/visibility/channel → 发布新不可变Release（不撤销旧版）→ 唯一安装版客户端正式检查更新/升级 → 核对Release ID/hash及安装schema → 安全隔离的模型/UI验收。出现新增权限或条款时交用户确认。

## 图片附件修复验收（2026-10-14追加）

2026-10-14主聊天/Task图片附件追加：这是原生客户端与共享Host传输修复，不是插件升级。主聊天新增只读附件伴随能力，Task附件读取提供实际模型图片输入；简单截图拟稿默认直接回答，明确要求后台Task时仍编排。正式App已原位重装且只运行一份，真实模型两条流程均通过，后台Task实际开始回调与最终草稿回写已观察。普通提问没有创建Task，Task请求实际包含图片，未使用Computer Use或发送邮件。详细根因、回归与Run记录见[图片附件修复验收](main-chat-image-attachment-fix.md)。此结果不替代上述Browser/CU等插件尚未完成的业务验收。

## Windows对齐说明

2026-10-14五类其他插件交叉测试追加：实际安装包完整性/Skill gate/MCP业务通过，原生UI覆盖与真实模型上游500阻塞分别记录；本次结果不能覆盖历史未完成事项。版本与详细证据见[其他插件交叉验收](plugin-cross-check-20261014.md)。

Windows native/UI本轮不修改。开发需对齐：activation附加Host协议（snake_case字段 `activate_tool`、`list_resources_tool`、`read_resource_tool`、`reference_argument`、`resource_path_argument`、`legacy_tool_mapping`、`guidance`）；保留冻结Skill原文与hash，evidence仍由Host维护；实机测试读取真实安装完整性记录、按实际工具schema校验参数，CI无UI catalog检查不得静默skip。PPT color/size为共享插件能力，旧Release未提供字段时不得编造参数。
