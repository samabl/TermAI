# ADR-0006：插件 ABI、运行时与 PTY 边界

- 状态：Accepted
- 日期：2025-01-01
- 决策者：Orchestrator（综合 10 角色评审）
- 关联：AR-07、AR-08；DC-26、DC-28、DC-38、DC-39、DC-40；HARNESS §0.4、§8.1、§10

## 背景与问题
1. 分歧：01 与 09 一致主张 **WASM 唯一 ABI**；04 硬否决插件拦截/改写 PTY 字节；08 指出「WASM 即沙箱」不成立，必须叠加**进程隔离与签名**；05 主张「声明式优先 + sandboxed iframe 兜底」；03 担心两套视觉漂移。
2. 硬边界：插件不得写 PTY/输出流、不得帧内任意绘制、不得持有 Node/原生句柄、不得静默网络访问（§0.4 第 4 条、§10.9/§10.10）。
3. 门禁：插件逃逸用例 0 成功；VT/PTY/IPC/插件消息 fuzz 24h 无 crash（§8.1）；TTFHW ≤5min 且热重载 p95 ≤300ms（§8.2、R10）。
4. 商业侧：插件市场 v1 零抽成、无 DRM（AR-10），因此安全门槛必须由技术而非经济手段承担。

## 可选方案（至少 2 个，含被否决项）
| 方案 | 主张方 | 否决 / 采纳理由 |
| --- | --- | --- |
| A 第三方原生动态库（dylib）ABI | 生态便利派 | 否决。无沙箱、ABI 脆弱、供应链风险不可控；列为 Anti-features §10.9 |
| B 纯进程内 WASM 沙箱 | 09 初版 | 否决为**唯一**手段。08 判定「WASM 即沙箱」不成立；需叠加独立进程 + 签名 + 权限清单 |
| C WIT + WASM Component，宿主独立进程；trusted subprocess 为白名单第二形态 | Orchestrator | **采纳**（AR-07） |

## 决策
1. **唯一 ABI = WIT + WASM Component**（语言中立、WASI 零 ambient authority）；第三方原生 dylib 一律拒绝。
2. **第二种形态 = trusted subprocess**，白名单限于 git/docker/kubectl/云 CLI，须用户**显式标记**，不进入市场普通分类。
3. **插件宿主是独立进程**（满足 08），WASM 是进程内子隔离层；**签名 + 权限清单强制**。
4. 插件对终端只有**只读镜像 + 显式注入队列 + 声明式装饰**（cell range + style token + z-order 枚举）；**永不开放帧内任意绘制，永不写 PTY/输出流**（采纳 04/09）。
5. **UI 能力分 Tier**：v1 只开放 **Tier1 声明式 view schema**（插件提交 JSON，Host 渲染）；**Tier2 隔离源 iframe** 需安全红队通过后开放（门禁：逃逸用例 0 成功 + CSP 拦截 100%）。
6. 扩展点三级：Tier1 冻结开放 / Tier2 试验 / **Tier3 永禁**（帧内绘制、直改 VT、直写输出流）（DC-39）。
7. 运行时保障：宿主**按需启动**，空载预算 ≤80MB（不占用核心 120MB 基线），每插件独立 wasm store + watchdog，3 次崩溃进 safe mode（DC-38）。
8. 兼容与治理：兼容 N-2 minor + 6 个月 deprecation + codemod；破坏性 WIT 变更需 RFC + 2 maintainer（DC-40）。

## 理由
1. 终端正确性与延迟下界不能由第三方抵押：只读镜像 + 声明式装饰把插件的能力上限压到不可能破坏网格语义。
2. 双形态（WASM / trusted subprocess）承认现实：git/docker/云 CLI 生态无法纯 WASM 化，用显式标记换取能力覆盖，同时保留审计与权限清单。
3. 先证明安全边界、再开放表达力（Tier1 → Tier2），与 A1（终端是信任基础设施）一致。

## 后果（正面 / 负面 / 需要接受的代价）
- 正面：崩溃隔离 100%；逃逸面可控；视觉不漂移（token 子集 + 组件白名单）；市场可零特权发布。
- 负面：WASM DX 可能劝退作者（R10）；Tier1 表达力有限；trusted subprocess 是受控的信任缺口。
- 需要接受的代价：
  1. componentize-js DX 指标未达 TTFHW ≤5min / 热重载 p95 ≤300ms 时，启用**每插件 V8 isolate 第二运行时**（P2 末门禁）。
  2. 必须维护 WIT 版本矩阵、签名与吊销链路（远程吊销 ≤1h，R9）。
  3. 插件 UI 只能使用 token 子集与组件白名单，作者自由度被主动限制。

## 反方记录与复议条件
- **反方（09 初版，保留原文）**：进程内 WASM 已经足够，独立进程带来 IPC 与启动成本，拖慢热重载与 TTFHW。
- **反方（05，保留原文）**：两套 UI（声明式 schema 与 iframe）会造成视觉与交互割裂；主张尽早统一表达力。
- **复议触发条件**：
  1. Tier2 iframe：安全红队通过（**逃逸用例 0 成功 + CSP 拦截 100%**）后开放（OQ-11，P3）。
  2. side-load 自签名插件：允许，但永久标记 Untrusted 且禁用 subprocess/secrets（OQ-12，P3）。
  3. 若宿主独立进程实测使 TTFHW >5min 或热重载 p95 >300ms，则按 P2 门禁启用第二运行时，**不因此放弃进程隔离**。
- 不可放宽：Tier3 永禁；插件永不写 PTY/输出流、永不帧内绘制。

## 关联决策（DC-xx）与实现位置
| 决策 | 内容 | 实现位置 |
| --- | --- | --- |
| DC-38 | 宿主独立进程 + wasm store + watchdog + safe mode | plugin-host |
| DC-39 | 扩展点三级（Tier3 永禁） | plugin-host（capability gate） |
| DC-40 | N-2 minor 兼容 + deprecation + codemod | WIT 版本目录与迁移脚本 |
| DC-26 / DC-28 | 工具 ABI、MCP 沙箱与二次校验 | plugin-host / termai-agent |
| AR-08 | Tier1 声明式 view schema | plugin-ui-sdk（JSON schema + Host 渲染器） |
