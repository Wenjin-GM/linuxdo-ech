# 在 Android 原生 App 内嵌 ECH 直连 linux.do（纯端上、无后端）

## 1. 状态与适用范围

这是未编译、未验证的 Rust/Kotlin 集成概念代码，不是可直接安装的 APK，也不是完整的 Gradle 工程。
目标是在原生 Android App 内研究 ECH 连接与本地认证流程，无自建后端。
浏览器挑战和 ECH 是否可用取决于站点、客户端及网络，不能保证通过挑战或完整抓取。
本文不代表已验证的 Android 平台或 Cloudflare 支持声明。

## 2. 架构与数据流

```
 业务层 (Kotlin)
 ├─ WebViewAuthActivity  ── 挑战/登录入口（JS、cookie、UA 一致）
 └─ LinuxdoRepository    ── 日常 /latest.json 翻页取数
        │  统一配置 proxy = 127.0.0.1:17840
        ▼
 Rust 本地代理 (liblinuxdo_ech_core.so)
 ├─ 客户端侧：本地 CA 终止 TLS（MITM）
 └─ 服务器侧：rustls ECH 加密 SNI
        │  DoH 多端点冗余 + 重试 → 真实 Cloudflare IP
        ▼
 Cloudflare（linux.do）
```

- 客户端到代理是本地回环，DPI 看不到外网目标；
- 代理到 Cloudflare 的 ClientHello 中 SNI 被 ECH 加密，DPI 无法识别 linux.do；
- 挑战 JS 在 WebView 内执行，通过后 `cf_clearance` 与登录态 `_t` 写入 CookieManager，OkHttp 复用。

## 3. 目录结构

```
android-embedded/
├─ rust-core/                 Rust 本地 ECH 代理核心
│  ├─ Cargo.toml
│  └─ src/{lib,proxy,doh,cert}.rs
└─ android/                   Kotlin 端
   ├─ EchProxy.kt             JNI 桥接 + 代理单例
   ├─ network_security_config.xml
   ├─ WebViewAuthActivity.kt  挑战/登录
   ├─ LinuxdoRepository.kt    翻页取数 + 挑战回退
   └─ NewsApp.kt              Application 初始化
```

## 4. 本地证书与认证：尚未完成的设计

当前 `cert.rs` 每次启动生成随机 CA，而 XML 引用了未提供的 `res/raw/ech_local_ca`。
两者尚未对接，WebView/OkHttp 信任链也没有完成验证。

- 不要把 CA 私钥、真实 cookie、User API Key 或固定共享密钥提交到仓库或打包进公开 APK。
- 不要安装到系统证书库，不要跳过服务器证书验证，不要在 SSL 错误回调中调用 proceed。
- 需重新设计每次安装独立、仅限 App 的安全信任方案；完成前不得用于真实账户登录。
- Cookie 当前只存在进程内；持久化前需明确加密、备份与清理策略。
- 代理需补齐目标域名限制、生命周期、超时及内存边界，并验证登录跳转不会泄露凭据。

仓库不包含 CA 证书或私钥，也不提供生成共享 CA 并嵌入客户端的发布步骤。

## 5. 编译 Rust 核心为 .so

```bash
# 安装工具链
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android

# 让 NDK 工具链（含 clang/cmake）进入 PATH，aws-lc-rs 交叉编译依赖它们
export ANDROID_NDK_HOME=/path/to/Android/Sdk/ndk/<version>

cargo build --release --target aarch64-linux-android
```

产物放入工程（按 ABI）：

```
app/src/main/jniLibs/arm64-v8a/liblinuxdo_ech_core.so
app/src/main/jniLibs/armeabi-v7a/liblinuxdo_ech_core.so
app/src/main/jniLibs/x86_64/liblinuxdo_ech_core.so
```

## 6. Android 工程配置

`build.gradle(.kts)` 依赖：

```kotlin
implementation("androidx.webkit:webkit:1.12.1")   // ProxyController / ProxyConfig
implementation("com.squareup.okhttp3:okhttp:4.12.0")
implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.8.1")
implementation("androidx.appcompat:appcompat:1.7.0")
```

`AndroidManifest.xml`：

```xml
<uses-permission android:name="android.permission.INTERNET" />
<application
    android:name=".NewsApp"
    android:networkSecurityConfig="@xml/network_security_config" ...>
    <activity android:name="com.linuxdo.news.core.WebViewAuthActivity" />
</application>
```

并在 `build.gradle` 限定 ABI（可选，减小体积）：

```kotlin
ndk { abiFilters += listOf("arm64-v8a", "armeabi-v7a", "x86_64") }
```

## 7. 拟议运行流程（未验证）

1. `NewsApp.onCreate` 调 `EchProxy.start`，后台启动本地代理。
2. 首次进入，打开 `WebViewAuthActivity`：WebView 经代理加载 linux.do，自动过挑战；用户在页面内登录。
3. 检测到 `cf_clearance`（及登录态 `_t`）后写回 `AuthState`，关闭界面。
4. `LinuxdoRepository.fetchToday()` 经代理 + cookie/UA 请求 `latest.json`，per_page=100 翻页，
   以本地当天零点为停止边界（边界与分页完整性尚待验证）。
5. 若 OkHttp 再次收到挑战页，抛 `ChallengeRequiredException`，UI 重新拉起 WebView 刷新后重试。

## 8. 可选：Discourse User API Key（比 cookie 更适合 App）

Discourse 官方为移动 App 提供 User API Key 流程，拿到后用 `User-Api-Key` 头请求，无需维护 cookie：

1. WebView 内打开
   `https://linux.do/user-api-key/new?application_name=News&client_id=<随机>&scopes=read`
2. 用户确认后，Discourse 回调一个一次性 URL，App 拦截并解析出 key。
3. 之后请求带 `User-Api-Key: <key>` 与固定 User-Agent。

是否对 linux.do 开放需实测；若管理员限制，则回退到第 7 节的 cookie 登录。

## 9. 踩坑与限制

- **DoH 间歇干扰**：`doh.rs` 已配置多端点 + 每端点重试；DoH 域名本身靠系统解析，需确保其未被污染。
- **TLS 指纹**：所有请求都经同一 Rust 代理，CF 看到的外层指纹一致，利于 `cf_clearance` 复用；
  不要让部分请求绕过代理直连。
- **UA 必须全程一致**：WebView 与 OkHttp 用同一 UA（`WebSettings.getDefaultUserAgent`），中途改 UA 会导致挑战失败。
- **明文回环**：到 127.0.0.1 的代理连接是明文 HTTP，已在 network security config 单独放行；仅回环，不外泄。
- **HTTP/2 与连接池**：当前核心为精简 HTTP/1.1，新闻场景足够；高并发可在 `proxy.rs` 增加 h2 与上游连接池。
- **合规**：仅用于个人/合规的内容获取，控制请求频率，遵守站点条款。

## 10. 验证范围

本次发布仅完成源码隐私检查与整理，没有编译或运行 Android/Rust 代码。
尚需核对依赖 API、Hyper 服务端特性、JNI、停止逻辑、代理回调、分页起点、认证跳转、
本地 CA 信任与系统版本兼容性。不得把这些示例称为生产可用或真机验证通过。
