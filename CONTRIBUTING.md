# 参与贡献

感谢你参与主观排序器的开发。提交代码前，请先确认修改范围明确、不会包含真实用户数据，并通过本地检查。

仓库地址：<https://github.com/Pan1corn/PersonalRanker>

## 开发环境

- Windows 10/11；
- Node.js 22.12 或更高版本、npm 10 或更高版本；
- Rust stable，并安装 `rustfmt` 和 `clippy`；
- Tauri 2 在 Windows 上所需的 WebView2 和 C++ 构建工具。

安装依赖并启动：

```powershell
npm install
npm run tauri dev
```

## 提交前检查

```powershell
npm run format:check
npm run lint
npm test
npm run build

Set-Location src-tauri
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

GitHub Actions 会在推送到 `main` 或创建拉取请求时重复执行这些检查。

## 数据库修改规则

`src-tauri/src/migrations/10000_v1_0_0_baseline.sql` 是首次公开版本的不可变基线。不要为了兼容开发预览版而重新加入旧迁移或在打开项目时静默修补表结构。

未来确实需要改变数据库结构时，应同时：

1. 明确新的数据格式版本及支持范围；
2. 提供事务化、可测试的升级路径，或明确拒绝旧格式；
3. 添加新建项目、升级失败回滚和不受支持格式的测试；
4. 更新 README、CHANGELOG 和应用内更新说明。

## 数据与隐私

- 不要提交 `.subject-sort` 项目、用户导入文件、数据库、日志或本地构建产物；
- 测试应使用代码生成的临时目录和虚构数据；
- 截图、错误信息和测试夹具中不得包含个人路径、邮箱、令牌或真实内容。

## 拉取请求

请让每个拉取请求只解决一个主题，并在说明中写明行为变化、测试结果、数据库兼容性影响和必要的界面截图。安全问题请遵循 [SECURITY.md](SECURITY.md)，不要先创建公开拉取请求。
