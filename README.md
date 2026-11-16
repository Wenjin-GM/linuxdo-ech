# linuxdo-ech

LINUX DO 当天话题导出、离线报告生成，以及 Android 原生 ECH 集成的实验源码。
无需作者账号或自建后端；Android 部分是未编译验证的概念代码，尚无可安装 APK。

## 文件

- `fetch_latest.js`：在你自己的已登录浏览器会话中分页读取话题并导出 JSON。
- `build_report.py`：使用 Python 标准库生成 JSON、CSV 与 HTML 报告。
- `cat_map.json`：公开分类名称映射。
- `android-embedded/`：Rust/Kotlin 集成示例与[限制说明](android-embedded/README_Android.md)。

## 生成本地报告

审阅 `fetch_latest.js` 后，在 linux.do 页面开发者工具中运行；站点权限和访问限制仍适用。
将下载的 `latest_YYYY-MM-DD.json` 放在本目录，再运行：

```sh
python build_report.py latest_YYYY-MM-DD.json
```

报告按 UTC+8 的创建日期筛选；浏览器导出按浏览器本地日期确定分页边界，跨时区使用需自行核对。
分页上限、置顶话题、站点排序或失败响应可能影响完整性，脚本没有完整性保证。
仅处理你有权访问的数据，并自行遵守站点规则和请求频率要求。

## 隐私边界

仓库仅发布源码与说明，不包含真实帖子、用户名清单、抓取报告、hosts 备份、运行日志、
第三方可执行程序、私人工作记忆、证书、cookie、账号密码或 API 密钥。
下载的数据及生成报告仅供本地使用，已设置忽略规则。不要把登录后可见或含个人信息的数据公开提交。
`.gitignore` 不能阻止强制添加文件；每次推送前仍应检查暂存内容。

Android 示例未完成本地证书信任与安全认证设计，请勿用于真实账户登录。
本仓库没有修改系统 hosts、安装系统证书或关闭 TLS 校验的使用步骤。

## 来源

ECH 实验参考 Linux.do-Accelerator 的设计；本次没有转载其工具目录、二进制程序或完整代理源码。
第三方依赖的许可证以各依赖的上游声明为准。
