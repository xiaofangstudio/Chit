# scaffold —— Chit 实现工程

> 小方工作室 · Chit ｜ 对应 [docs/07](../docs/07-里程碑与验收.md) 的 **M0 骨架** + M1 部分能力

## 当前形态说明

按 [docs/03](../docs/03-技术选型与架构.md) 的**备选路线「Rust + windows-rs 自研」**实现（首选路线的 Notepad4 底座尚未克隆，且本机无 MSVC 编译器；Rust 路线是文档明确允许的正式备选）。产物同样是：

- **单文件 `Chit.exe`**，静态 CRT（`crt-static`），零 DLL 依赖
- 原生 Win32，无运行时、无 Web 技术
- 零联网（源码无任何网络符号）、零常驻（关窗即退）、全中文界面

## 构建

```powershell
powershell -ExecutionPolicy Bypass -File build.ps1
# 或
cargo +stable-x86_64-pc-windows-gnu build --release
```

产物：`target\x86_64-pc-windows-gnu\release\chit.exe`

## 目录

```
scaffold/
├── Cargo.toml            依赖仅 windows-rs（纯 API 绑定）
├── .cargo/config.toml    gnu 目标 + 静态 CRT + 自带链接器
├── build.ps1             构建脚本
├── src/
│   ├── main.rs           入口：DPI 感知、单实例、命令行打开
│   ├── app.rs            主窗口、命令表、菜单、查找替换、转到行
│   ├── ui.rs             自绘工具栏/标签栏/状态栏/查找条、线性图标
│   ├── edit.rs           标签页文档状态、多级撤销、行操作、换行符
│   ├── encoding.rs       编码探测与转换（BOM/UTF-8/GB18030/ANSI…）
│   ├── fileio.rs         原子写入、ini 配置、最近文件
│   └── consts.rs         命令表与常量
├── scripts/make-icons.py 超椭圆圆角图标生成（跨项目规范）
├── res/chit.ico          生成的应用图标（运行时嵌入，无 .rc 编译）
└── licenses/             MIT + 第三方署名（见 README）
```

## 已实现（对应 docs/02 模块）

- **A 文件**：新建 / 打开（多选）/ 保存 / 另存为 / 拖拽打开 / 命令行打开 / 最近文件 / 编码探测（A3 四步）
- **B 编辑**：快照式多级撤销（随标签独立，大文档退回单级）/ 剪贴板 / 删除行 / 复制行 / 上移行 / 下移行
- **C 标签**：多标签 / 未保存圆点 / 中键·双击·× 关闭 / 右键菜单（关闭其他、复制路径、资源管理器）
- **D 查找**：底部嵌入式查找条 / 计数 / 上一个下一个 / 区分大小写 / 替换与全部替换（正则留 M1）
- **E 编码与换行**：10 种编码转换 / 「以…重新解释」修乱码 / CRLF·LF·CR / 有损转换保护
- **F 视图**：自动换行 / 字号缩放（Ctrl+滚轮、Ctrl+加减号）/ 字体对话框 / 只读 / 全屏

## 尚未实现（按里程碑排期）

- 行号栏、正则查找替换（M1 后续）
- 深色模式、会话恢复、自动草稿（M2）
- 大文件模式（M2）
- 文件关联、安装包、干净卸载（M3/M4）

## 与文档的两处偏差（经用户确认）

1. **界面**：docs/05 原定「无工具栏、无品牌色」；用户 2026-11-10 明确要求「UI 要好看，像个简单的 Word」→ 采用自绘工具栏 + Word 蓝点缀的浅色主题。其余（无动画、原生对话框、全中文、系统语义）仍遵守。
2. **语言**：docs/03 首选 Notepad4 底座改造；因底座未克隆、本机无 MSVC，按 §2.2 备选 Rust 自研。底座路线仍可作为后续演进。
