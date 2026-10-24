# #17 / #23：累计来源接口与原分页意图

按[契约](rust-agent-ui-native-paged-pane-source-contract-20261025.md)完成 presentation 与 bindings 切片；关联 #27、PR #10。对照 ClientKit `c289bc9d33edcd83a782ac17927a8c65a027eccd` 的 RunChildHistoryObserver 生命周期及 SessionWindowCoordinator 的 frozen cursor/原generation检查；Tekes只读基线仍为 `8858ccd9d46590852ec7a0949aee289fc6798d70`。没有修改Swift、Kernel或TekesUI，公开SDK pin仍为 `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`。

新增默认关闭的 `BoundedChildPagedSourceWitness` / `ChildRun::transcript_with_bounded_paged_source_witness`，在原bounded mutex内记录实际Initial/Refresh/Earlier/PrefixValidation的typed请求响应、原读取epoch、merge/reset及有效分页状态。显式见证才冻结、重放核对原累计entries与head元数据；PrefixValidation不合并。普通refresh不复制累计entries维护审计，不增加RPC。head-only接口保持严格拒绝累计旧页。

来源最多128条、8 MiB保留编码量；超限/来源不完整只拒绝见证，正常投影继续；真实reset可从新head重开完整链。这不是瞬时内存峰值限制：捕获页及编码检查仍会临时克隆/序列化单页。独立review未发现阻断问题，并保留此资源边界建议；没有性能验收。

binding新增 `snapshot_with_bounded_paged_source_witness`，沿原snapshot路径分配同一次publication，检查精确parent/child、owner、source与epoch。`ChildPaneBinding::read_intent_with_paged_source_witness`和原read_intent共用原admission/claim/Earlier路径，随后仅做原有的一次head refresh，不先读普通snapshot再另取见证。重连旧Earlier拒绝；显式Refresh可以产生新epoch publication，同时逐页明确旧缓存的原epoch，不声称这些缓存全部经新连接重读。不声称网络字节或服务端事务一致。

验证结果：

| 范围 | 本次结果 |
| --- | --- |
| root默认 / presentation feature | 404/0/0；301/0/0 |
| UI默认 / bindings+demo feature | 784/0/1（既有ignore）；674/0/0 |
| 新binding定向 | 4/4，含原ReadOnlyTranscript控件产生的Earlier/Refresh |
| root/UI严格Clippy | 默认与feature均通过 |
| 格式、Rust ownership边界 | 通过 |
| 独立公开consumer | 默认与feature均实际构造/挂载原组件并退出0；新增见证future仅作公开API类型检查，不冒充已执行SEP方法 |

受控SEP fixture覆盖累计Earlier/head-only拒绝、单次刷新/publication、重复claim、原frontier/cut/revision、prefix验证不合并、gap/stale reset、owner退休和跨epoch拒绝、显式刷新保留旧epoch缓存，以及上限/恢复。主会话修正了候选中的默认cfg/无效snapshot字段与夹具的turn-step、页数、请求计数和锁作用域；失败日志保留，没有放宽产品门禁或跳过测试。

586个源码/脚本/锁输入在验证前后逐项一致；来源manifest、日志、两份consumer源/锁及归档程序位于SSD `.runtime-fixtures/acceptance-refresh-20261025/`。UI工作区解析8个SDK package/243个已审阅文件；独立consumer实际解析其中6个相同公开package，不含render/platform-macos，manifest路径与已验证UI闭包逐项匹配；registry依赖亦匹配既有UI锁文件。新增dev-dependency仅为bindings测试消费现有controls，锁文件仅增加这一依赖边，无新版本/override。复用共享target，未新建target、删除缓存0字节。

**尚未完成：本次真实Native同一pane累计分页、重连与阅读位置收据。** 当前原组件产生intent与异步读取的结果来自受控fixture；两个独立程序的构造/API检查不是完整行为验收。桌面、物理IME、OS AX、ProcessContainer、完整View parity及性能仍未验收。现有矩阵继续固定91129d7，不自动把旧协议或本次接口测试升级为新代码的真实协议通过；#17/#23/#27保持开放，#53与Kernel#86不豁免，不合并/tag/发布。

审计提交9396293的push40137200156/PR40137204158已终态核验，唯一失败仍#53；这是前一份审计代码CI，不是本实现的新head CI。
