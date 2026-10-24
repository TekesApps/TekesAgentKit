# Sidebar 多 endpoint demo 宿主接线

Issue #22 的非桌面宿主接线说明。宿主仅组合公开 `SidebarBinding` / `SidebarMultiEndpointBinding`，不复制 inventory、selected-read 或 action 状态机。

以下初批说明保留其历史范围；当前多端配置和 provider 搜索已接通，见文末及[集成收据](rust-agent-ui-multi-search-results-20261024.md)。

## 宿主适配

`SidebarHostBinding::new(agent, endpoint_ids)` 从同一个 `SharedAgent` 取 endpoint/runtime。一个 endpoint 使用既有单端 `SidebarBinding`；多个 endpoint 使用固定集合 `SidebarMultiEndpointBinding`。适配器只归一化 publication、保留 owner 身份并把选中读取/动作调用交回对应原 binding。更换 reader 会提升 `ComponentDemo` owner epoch、清除本地 pending gestures 和旧搜索状态；publication 必须属于当前 adapter owner，旧 owner 即使 revision 重启到 1 也不能覆盖新 source。view 自身的 Sidebar revision 持续递增，不因 owner replacement 回退。

原生入口现在由本次 host 实际注册的 endpoint ID 集合创建 adapter，并为 `OpenSession`、新会话的 preset/catalog 与创建、子历史打开使用请求自身 endpoint 对应的 runtime；重连在派发时捕获当前 run endpoint。目标端未注册时明确失败，不回退到首端。当前 endpoint config 仍是单 connector，默认 selftest/fixture 也只注册一个 endpoint；scope 列表本身不能证明多个独立 harness/backend 已配置。因此自动选择兼容单端 adapter，不改变原查询测试行为；多端行为通过两个隔离 fixture runtime 的离屏 `ComponentDemo` 集成夹具验证。没有新增多 endpoint Native 窗口或桌面验收声明。

SideChat 仍只有初始化主 endpoint 的 controller。面板在其他 endpoint 下显式投影 `supports_side_chat=false`，native handler 也会拒绝跨 endpoint 的创建、打开、关闭旧命令；没有把请求路由给首端，也没有声称多端 SideChat 已实现。Sidebar owner 更换还会递增 navigation revision，使旧 `OpenSession` 回包不能覆盖新 owner 下的选择。

## 搜索和文件夹

跨 endpoint 全局搜索未实现。多端库存下，demo host 拒绝挂载单端 query publication，并显示 host 侧“没有跨端搜索来源”的提示；不会把 A 端 query 结果混入 B 端 inventory，也不会清空 B 端 rows。单端 adapter 保留现有搜索行为。合并 inventory 使用现有 Sidebar 的全局排序、过滤和分页；folder catalog 仍以 endpoint-qualified folder key 区分相同路径。

## 回归范围

`ui/demo/tests/sidebar_multi_host.rs` 通过两个隔离 endpoint（都含 session ID `a`）构造实际 `ComponentDemo`；验证合并行及文件夹身份、对 B 端的 selected read/reveal 和 archive action 只影响 B、单端 SideChat owner 在 B 下禁用、混合切换到多端 host 后旧单端及旧聚合 publication 被拒绝、旧 selected-read 与导航回包失效、owner replacement 清除 pending gesture、旧 action completion 不能清理新 owner 的 pending 或提交导航。

主会话应执行：

```sh
CARGO_TARGET_DIR=/tmp/agentkit-layout-text-292-target CARGO_INCREMENTAL=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test --manifest-path ui/Cargo.toml -p tekes-agent-ui-demo --test sidebar_multi_host --locked --offline
CARGO_TARGET_DIR=/tmp/agentkit-layout-text-292-target CARGO_INCREMENTAL=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test --manifest-path ui/Cargo.toml -p tekes-agent-ui-demo --test sidebar_multi --locked --offline
```

本子会话仅运行 `rustfmt --check` 与 `git diff --check`，未运行 Cargo、桌面、第二 consumer 或 CI。跨端全局搜索、多端 SideChat、外部/真实三 harness 同窗端点配置、observer 同实例恢复、原生辅助功能和窗口验收仍未覆盖；这些结果不能由离屏夹具推断。

## 主会话复核修正与验证边界

会话创建、初始 preset 目录、子历史、OpenSession 和重连均按原 request/scope 对应 endpoint 选择 runtime；没有匹配项就报错，不回退首端。SideChat 当前仅安装一个 endpoint 的 controller：其他 endpoint 的面板 `supports_side_chat=false`，native handler 对不匹配旧命令同样拒绝。这是显式能力限制，不是多端 SideChat 的完整实现。

主会话修正了测试 fixture 的 endpoint identity 和旧 native 查询探针的 action API 调用。新增宿主测试直接捕获共享 action definition 并检查真实 fixture IO 的 endpoint 归属；pending fixture 显式安装本地门禁，再验证替换 owner 后退休、旧完成不能清除新 pending。该测试不声称实际弹出菜单或完成原生/AX操作；菜单交互有独立既有组件 suite。选中/reveal 断言验证目标身份，不等于 SDK 实际像素可见性验收。

publication owner 现为不可变封装：公开读取仅提供 `Deref`，没有 `DerefMut` 或 wrapper Clone；复制出来的普通 `SidebarPublication` 不携带 owner。既有 restore fixture 通过受限 `with_restore_fixture_projection` 调整加载/错误和单调 revision、隐藏原行或标记 stale，不能修改标题、身份、动作、活动时间或新增行；回归包含伪造行拒绝与合法状态正例。

## 主会话运行结果

UI workspace 全套 637 passed / 0 failed / 1 项原有忽略（TekesUI #49），日志 `.runtime-fixtures/sidebar-host-20261024/ui-workspace-tests.log`。随后仅收紧 publication 的可变性并保留只读字段访问，最终源码的多端宿主定向测试再次通过（1 项），日志 `sidebar-host-final-immutable-tests.log`；严格 all-target Clippy、格式/边界和公开 consumer 由统一收据记录。没有把全套测试扩报为不可变封装之后的再次全套执行。

## 后续设计：实际多端配置入口（Issue #22）

现有 `EndpointConfiguration` 保持单端/两会话兼容；demo host 增加显式 `HostEndpointConfiguration`，接收原单端对象或仅含 `endpoints` 数组的对象。先纯解析全部配置并拒绝空数组、重复 endpoint ID、额外顶层字段和坏子项，再依次构造各自 connector；每个 endpoint 使用自己的 kind/connector/runtime，不把同一个 connector 克隆为多个后端。每端原有两个 session scope 的顺序与 ID 保留，host 按配置顺序组合初始 tabs，选中补读/动作沿用原多端 binding。配置/凭据不实现 Debug、不写日志，不自动创建 session或重试 mutation；未授权桌面启动。

主会话集成 native dispatch；新配置子会话只改配置解析/connector transfer 与专属单元测试，集中构建仍由主会话负责。真实多个 harness 配置运行与原生验收将单列证据，不能由纯解析测试推断。跨端搜索另经共享 binding 设计，不加入宿主投影补丁。

## 实际多端配置入口实施

`HostEndpointConfiguration` 保留原 `EndpointConfiguration` 的纯校验和单端API；没有把 Native URL 校验转交 DSH，也没有新增连接期兼容规则。原生组件入口接受多端配置并逐端消费自己的 `ConfiguredConnector`，原顺序展开所有scope；单端专属proof/selftest拒绝聚合配置，避免主端统计被误报为多端证据。Summary打开子代理按原parent endpoint选择Native有界/其他独立历史布局，不再使用首端family。默认单端模式、共享偏好与首端SideChat限制保留。

配置解析5项、原单端6项、pane uploads7项、多端宿主1项通过（此前日志 `host-config-tests.log`）；Native Backend随后恢复为原纯构造API，最终校验另记。不运行桌面或创建真实session；纯配置和编译不能证明真实三harness同窗。

## 当前多端 provider 搜索接线

多端配置入口与 SidebarMultiSearchBinding 已接通原生组件 demo；--sidebar-query 为固定端集合创建共享 reader，原 typed Retry 只发回原失败端。宿主不合并 hit、不伪造全局 single publication。双端 fixture 已做独立协议、ComponentDemo、第二 consumer 和自有 renderer 离屏验证；这里的“原生入口接通”不代表启动过窗口或真实多 harness 运行。首端 SideChat 能力限制仍然存在；Mirror FTS/pins 差异、实际像素 reveal、observer 恢复与远端搜索续页继续单列。
