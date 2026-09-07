# PersonalRanker（主观排序器）

[![CI](https://github.com/Pan1corn/PersonalRanker/actions/workflows/ci.yml/badge.svg)](https://github.com/Pan1corn/PersonalRanker/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Pan1corn/PersonalRanker?display_name=tag)](https://github.com/Pan1corn/PersonalRanker/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)

PersonalRanker（主观排序器）是面向 Windows 10/11 的本地单机排序工具。导入一组条目后，可以为数据表选择拖拽排序、1v1 矩阵排序、1v1 传递排序或滑杆排序，并将排名、评分或完整结果写入项目工作副本或导出为独立文件。

当前正式版本为 **1.0.0**。应用数据默认保存在本机，不会上传到远程服务；只有用户明确开启“加载网络图片”后，应用才会访问数据中的网络图片地址。

## 1.0.0 正式版摘要

- 提供拖拽排序、1v1 矩阵排序、1v1 传递排序和滑杆排序四种判断方式。
- 新增矩阵比较得分表、传递排序双进度与当前顺序预览。
- 支持修改排序标准、重命名并列组、删除任务，以及基于锁定结果继续创建排序任务。
- 修复线性评分预览与写入值未严格遵循小数位数的问题。
- 启用统一品牌 Logo，并用于首页、应用窗口、桌面快捷方式和安装包。

## 已实现功能

### 项目与数据导入

- 使用目录型 `.subject-sort` 项目保存元数据、SQLite 工作副本、只读来源副本和历史快照。
- 支持 TXT、CSV、JSON、Markdown 表格、Markdown 列表和直接粘贴多行文本。
- CSV 支持 UTF-8 BOM、空字段名补全和重复字段名消歧；解析错误会标明行号。
- JSON 支持发现并选择根对象数组或嵌套对象数组。
- 导入时自动移除空条目，可选择去重，并保留原始序号。
- 提供条目、字段、重复项和空条目统计，以及字段类型、空值数、唯一值数和样例预览。
- 可配置主标识字段和多个辅助标识字段；重新打开项目后会恢复数据表与字段配置。
- 对图片字段支持本地路径、显式启用的网络图片，以及按相对路径或唯一文件名从图片文件夹补充匹配。

### 排序任务管理

- 同一数据表可创建多个相互独立的排序任务，并在项目侧栏和各工作台之间直接切换。
- 每个任务可设置名称、排序标准，并从四种排序方式中进行选择。
- 初始顺序可沿用导入顺序、按数字或日期字段升降序预排，或复制同一数据表中已确认任务的结果。
- 可删除排序任务或整个数据表；删除前会显示不可撤销警告，并清理关联数据。
- 排序结果确认后会锁定编辑；项目页、拖拽工作台、1v1 工作台和结果页均显示锁定状态并提供解锁入口。

### 自动保存与历史快照

- 拖拽、比较、结果确认、排名和评分等关键操作会记录自动保存状态。
- 异常关闭后重新打开项目时，会提示已恢复上次提交的状态。
- 关键业务节点会创建完整项目快照，最多保留最近 50 个。
- 可从历史快照恢复项目；恢复前会自动保存当前状态作为安全备份。

### 拖拽排序

- 使用虚拟列表按可视区域渲染，支持 1,000 条数据的滚动与排序操作。
- 支持拖动排序、置顶、置底、全文搜索、跳转到指定排名，以及折叠或展开辅助字段。
- 可将条目或排名组合并为并列组，也可拆分并列组中的条目和重命名并列组。
- 可选取连续排名区间进行局部重排，再一次性写回完整顺序。
- 拖拽与既有 1v1 判断冲突时，可选择以拖拽为准并同步关系、仅临时移动或取消。
- 排序操作即时保存，并支持持久化的撤销与重做。

### 1v1 传递排序

- 使用二分插入逐步定位条目，避免进行全量两两比较。
- 支持左侧靠前、右侧靠前、二者并列、跳过当前比较和撤销上一判断。
- 支持 `A`/`←`、`D`/`→`、`W`/`↑`、`S`/`↓` 和 `Ctrl+Z` 快捷键。
- 显示已完成判断、预计剩余和待定位条目数量；每次判断和算法状态都会保存，退出后可继续。
- 完成后生成完整排序结果，并可进入预览和确认流程。

### 结果、排名、评分与导出

- 确认前检查条目完整性、未解决比较、重复内部 ID 和无效条目。
- 确认结果时创建只读快照并锁定工作台；解锁后仍保留最近一次确认快照。
- 可把竞赛排名、密集排名或顺序排名写入项目工作副本，并标记已写入的字段和时间。
- 支持线性评分和等量分档；并列组可取所占位置的平均分、最高分或最低分。
- 评分正式写入前会预览原字段值和新分数，评分配置会自动保存。
- 支持导出 CSV、JSON、Markdown 表格、Markdown 列表和纯文本。
- 导出时可选择最终排名或原始顺序、全部或指定字段，并按需包含最终排名、原始索引和已写入的评分字段。
- 目标文件已存在时可选择覆盖、另存为或取消；导出不会修改原始只读来源文件。

## 安装与首次使用

1. 打开 [GitHub Releases](https://github.com/Pan1corn/PersonalRanker/releases)，选择最新的 `v1.0.0` 版本。
2. 下载文件名以 `_x64-setup.exe` 结尾的 Windows 安装包。
3. 运行安装包并按提示完成安装。安装包尚未进行商业代码签名时，Windows 可能显示“未知发布者”；请确认文件来自本项目的 GitHub Releases 后再继续。
4. 启动应用，选择“新建排序项目”，为项目命名并选择保存目录。
5. 导入 TXT、CSV、JSON 或 Markdown 数据，确认标识字段后创建排序任务。

项目是一个以 `.subject-sort` 结尾的文件夹。移动或备份项目时，请复制整个文件夹，而不是只复制其中的 `project.sqlite`。

### 项目兼容性

v1.0.0 是首次公开的项目数据库基线，内部数据格式为 `10000`。仅支持由正式版 v1.0.0 创建的项目；开发测试阶段及 v1.0.0 之前版本创建的项目不会被自动迁移或修改。打开不受支持的项目时，应用会明确报错。

升级应用前建议先备份完整的 `.subject-sort` 项目文件夹。不要手动编辑 `metadata.json`、`project.sqlite` 或历史快照来绕过版本检查。

## 项目结构

```text
.
├─ .github/                    CI、依赖更新与 Issue 模板
├─ src/                         React + TypeScript 前端
│  ├─ components/              通用界面组件
│  ├─ config/                  应用级配置
│  ├─ content/                 应用内文档内容
│  ├─ features/
│  │  ├─ import/               数据导入、预览与字段配置
│  │  ├─ media/                本地与网络图片预览
│  │  ├─ projects/             项目工作区、自动保存与历史快照
│  │  └─ sort-tasks/           任务创建、拖拽、1v1、结果与导出
│  ├─ lib/                     错误规范化等基础工具
│  ├─ App.tsx                  应用入口界面
│  └─ styles.css               全局样式
├─ src-tauri/                   Tauri 2 / Rust 桌面后端
│  ├─ capabilities/            桌面权限配置
│  ├─ icons/                   应用与安装包图标
│  └─ src/
│     ├─ commands/             暴露给前端的 Tauri 命令
│     ├─ domain/               项目、导入、排序和结果领域模型
│     ├─ migrations/           SQLite v1.0.0 数据库基线
│     ├─ repository/           SQLite 持久化访问
│     └─ services/             导入、排序、评分、快照等业务逻辑
├─ README.md                   当前版本说明与开发入口
├─ CHANGELOG.md                面向用户的版本变化
├─ CONTRIBUTING.md             贡献流程和数据库修改规则
├─ SECURITY.md                 安全支持与私密报告方式
├─ LICENSE                     MIT 开源许可证
├─ package.json                前端依赖、版本与脚本
└─ vite.config.ts              Vite 与 Vitest 配置
```

前端测试与被测模块放在同一目录，文件名使用 `*.test.ts` 或 `*.test.tsx`；Rust 单元测试位于对应源码模块内。`dist/` 和 `src-tauri/target/` 是构建产物目录。

## 开发与检查

克隆仓库并安装依赖：

```powershell
git clone https://github.com/Pan1corn/PersonalRanker.git
Set-Location PersonalRanker
npm install
```

安装依赖并运行前端检查：

```powershell
npm.cmd install
npm.cmd run lint
npm.cmd run format:check
npm.cmd test
npm.cmd run build
```

启动桌面开发环境：

```powershell
npm.cmd run tauri dev
```

运行 Rust 检查：

```powershell
Set-Location src-tauri
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

所有提交和拉取请求都会通过 GitHub Actions 执行相同的前端与 Rust 检查。参与开发前请阅读 [CONTRIBUTING.md](CONTRIBUTING.md)，版本变化记录见 [CHANGELOG.md](CHANGELOG.md)。

## 隐私与安全

- 项目数据默认仅保存在用户选择的本地目录，不包含遥测或云端同步。
- 网络图片默认禁用；启用后，图片服务器仍可能获得你的 IP 地址等常规网络请求信息。
- `.subject-sort` 项目可能包含原始数据副本、图片和历史快照，请勿将真实项目作为公开 Issue 附件上传。
- 安全漏洞请按照 [SECURITY.md](SECURITY.md) 私下报告，不要直接公开利用细节。

## 许可证

本项目基于 [MIT License](LICENSE) 开源。

项目主页：[github.com/Pan1corn/PersonalRanker](https://github.com/Pan1corn/PersonalRanker)
