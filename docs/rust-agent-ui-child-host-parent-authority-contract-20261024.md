# #23：实际宿主 child factory 父权限契约

基线 `3b2c89e`。前轮已验证实际 Pi/DSH 一层子 pane 原运行/失败与关闭；只读检查发现 actual OpenChild dispatcher 未使用 `for_run`，而是 scope-only factory。已有 bindings 继承测试不能替代宿主路径。本轮只修 AgentKit，不修改 TekesUI、ClientKit、Tekes 或 Kernel。

1. 实际 Summary/FamilyCatalog 产生的 child-open 必须在 dispatch 之前捕获其 direct parent 的原 `Arc<RunBinding>`，异步阶段不按 scope 换成新父。scope mismatch、未知或已退休父拒绝。
2. actual dispatcher 与 CPU 回归使用同一 host open helper。它从原父构造 child 并继承完整 ancestor authority；await 前后检查，父在读取中退休不得发布有效来源。
3. `for_run` 保持默认页大小兼容；新增显式页大小入口复用原有 bounded 校验。独立历史与 SameSession 的差别、只读及无 Composer/Send 不变。
4. 缓存复用同时要求 source 仍有效且属于捕获的同一父 binding。相同 scope 的替换父不能授予旧 source 新权限。选择 A→B→A 不等于退役，不删除合法缓存或原草稿。
5. 回归覆盖实际 helper 原父身份、祖先退休/迟到拒绝、缓存替换隔离和合法导航、bounded 页大小。用合成嵌套绑定测试实际 helper 的权限传播，不创造未存在的嵌套 UI 入口，不称真实嵌套桌面验收。

验证由主会话串行复用 SSD shared-target：定向红/绿、bindings/demo 相关测试、默认和 opt-in 严格检查、公共 consumer；根据改动拓宽。保留原失败与最终程序/输入证据。不为审计新建 target，不运行桌面、IME、OS AX 或重新触发授权。#23 其余真实动态 Task pointer、pane 恢复/分页与完整 UI 标准仍开放，#53 不豁免。
