[English](README.md) | **简体中文**

# Blue Archive Strategy Engine

`ba-strategy` 0.2.0 是一款在本地运行的 Rust 概率分析引擎，用于分析按顺序指定的
一到两个《蔚蓝档案》招募目标。它支持穷举分析、可复现的串行蒙特卡洛模拟、轨迹记录
与重放，以及精确分析与模拟结果的对比。

本项目是一款非官方的爱好者/研究工具，与 Nexon、Yostar、《蔚蓝档案》及其关联方
均无隶属关系，亦未获得其认可或背书，也不是官方信息来源。本仓库不包含受版权保护的
游戏素材。

随项目提供的数据均明确标记为暂定（provisional）数据。特别是，
`jp_2026_07_29_provisional_v2` 既未经独立核验，也不是官方的游戏规则说明。

## 构建与运行

项目使用的 Rust 1.95.0、rustfmt 和 Clippy 由
[`rust-toolchain.toml`](rust-toolchain.toml) 指定。构建时请使用仓库内已提交的
`Cargo.lock`。

```text
cargo build --workspace --locked

cargo run --locked -p ba-cli --bin ba-strategy -- \
  catalog list all --format json

cargo run --locked -p ba-cli --bin ba-strategy -- \
  --scenario-dir scenarios/examples analyze example_single_target_v2 --format json

cargo run --locked -p ba-cli --bin ba-strategy -- \
  scenario template --scenario-id generated_v2 \
  --ruleset jp_2026_07_29_provisional_v2 \
  --reward-schedule jp_2026_07_29_empty_v2 --target-count 2
```

原有命令 `validate`、`analyze`、`simulate` 和 `compare` 均继续可用。新增的本地
检查命令包括 `catalog list`、`catalog inspect`、`scenario explain` 和
`scenario template`。`catalog list`、`catalog inspect` 和 `scenario explain`
的 JSON 输出均包含明确的输出 schema 版本号。`scenario template` 会把合法的
schema v2 JSON 写入 stdout，默认先使用 10次招募券，再使用付费单次招募，招募总
次数上限为 200 次。

`--data-dir` 默认为 `./data`。指定 `--scenario-dir <PATH>` 后，`foo` 或
`foo.json` 这样的单独名称会解析为 `<PATH>/foo.json`；带有 `./`、`../` 的路径、
嵌套路径和绝对路径仍按显式路径处理。未指定 `--scenario-dir` 时，单独名称会从
`scenarios/golden/` 解析。

`validate --diagnostics` 在失败时会输出 diagnostics-schema-v1 格式的错误封装，
其中包含稳定的错误类别、代码和消息；如可用，还会包含指针、行号、列号和修正提示。
成功的验证报告会包含行为指纹和文档指纹。成功结果写入 stdout；失败时错误写入
stderr，stdout 不会留下任何可视为权威结果的输出。

| 退出码 | 含义 |
|---:|---|
| 0 | 成功，包括领域逻辑的正常终止结果 |
| 2 | 命令行用法错误 |
| 3 | JSON、schema 或领域验证错误 |
| 4 | `catalog`、文件系统或熵源 I/O 错误，或达到拒绝采样上限 |
| 5 | 引擎防护、算术、状态转移或概率不变量错误 |
| 70 | 未预期的类型化内部故障 |

## 输入与兼容性

`data/` 存放随项目提供的暂定运行时数据；`scenarios/examples/` 存放仅使用这些数据
的场景编写示例；`scenarios/golden/` 存放冻结的回归场景；`tests/fixtures/` 存放
合成及对抗性测试数据。`synthetic_custom_*` 测试夹具刻意采用与实际游戏玩法无关的
机制，绝不会作为运行时 `catalog` 数据或面向发布的示例。

所有输入文档均使用 schema 2。Schema 1 仅作为未投入使用的开发格式存在，现已不再接受。
成功的分析统一使用引擎语义版本 2 和结果 schema 2。详情请参阅
[`docs/SCHEMA_V2.md`](docs/SCHEMA_V2.md)。

V2 结果区分行为指纹与文档指纹。若仅更改 v2 的来源元数据（provenance），文档身份
和输出中的来源信息会随之改变；但机制、策略决策、精确分析和蒙特卡洛模拟的行为均
不受影响，每次运行的种子派生方式也不会改变。请参阅
[`docs/SCHEMA_V2.md`](docs/SCHEMA_V2.md)。

## 安全模型

在 Linux 和 Android 上，用户选定的环境根路径（`--data-dir`、`--scenario-dir`
或显式指定文档的父目录）本身可跟随一次符号链接，随后会固定所得的目录描述符。根
路径下的所有后代路径均相对于该描述符解析，且不再跟随符号链接。规则集和奖励计划
会从同一个经过校验的数据根目录版本中加载；如果检测到可观察的并发替换，加载将以
`catalog_generation_changed` 错误立即失败并拒绝返回结果，而不会返回混合了不同
版本规则集和奖励计划的 `catalog`。在其他平台上，安全加载会直接失败（fail closed），
不会降级到普通加载方式。这项机制只提供文件系统一致性边界；面对权限足够高的攻击者，
它并不提供密码学完整性保证。请参阅
[`docs/THREAT_MODEL.md`](docs/THREAT_MODEL.md)。

## 模型与限制

每个动作都是原子的：一旦锁定招募卡池，该动作就会完成其中的全部基础招募，即使
提前获得目标也不会中止；策略仅在动作边界重新评估。每个场景都必须显式指定正整数
招募上限；`funding_priority` 必须且只能是 `ticket_ten` 与 `paid_single` 各出现
一次的两种排列之一。请参阅
[`docs/STRATEGIES.md`](docs/STRATEGIES.md)。

单份文档的大小上限为 1 MiB，JSON 最大深度为 64；最多检查 512 个目录直接子项，
最多保留 256 个 JSON 候选文件。精确分析会枚举模型中所有概率非零的分支，不做概率
剪枝。引擎防护阈值的校准结果和特定环境下的基准测试观测值记录在
[`docs/CALIBRATION.md`](docs/CALIBRATION.md) 中。

## 工作区与发布

```text
ba-cli -> ba-engine -> ba-core
```

- `ba-core`：严格输入处理、安全的 `catalog` 加载、验证、指纹、策略和纯状态转移
  内核。
- `ba-engine`：精确概率传播、蒙特卡洛模拟、轨迹记录/重放、比较和结果投影。
- `ba-cli`：参数解析、路径解析、命令执行、渲染和错误映射。

本项目未发布到 crates.io；本实现不会执行打 Git 标签、推送、发布软件包或创建
Release 等操作。发布就绪条件及其最小权限边界请参阅
[`docs/RELEASING.md`](docs/RELEASING.md)。

贡献方式、安全问题报告流程以及 MIT/Apache-2.0 双重许可条款，分别见
[`CONTRIBUTING.md`](CONTRIBUTING.md)、[`SECURITY.md`](SECURITY.md)、
[`LICENSE-MIT`](LICENSE-MIT) 和 [`LICENSE-APACHE`](LICENSE-APACHE)。
