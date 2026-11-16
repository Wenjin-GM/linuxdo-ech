package com.linuxdo.news.core

import android.annotation.SuppressLint
import android.os.Bundle
import android.webkit.CookieManager
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.appcompat.app.AppCompatActivity
import androidx.webkit.ProxyConfig
import androidx.webkit.ProxyController

/**
 * 用 WebView 完成 Cloudflare JS 挑战与（可选）登录。
 * WebView 自身不支持 ECH，故通过 androidx.webkit ProxyController 让其走本地 ECH 代理。
 * 成功后把 cookie（含 cf_clearance 与登录态 _t）写入 [AuthState] 供 OkHttp 复用。
 */
class WebViewAuthActivity : AppCompatActivity() {

    private lateinit var webView: WebView

    @SuppressLint("SetJavaScriptEnabled")
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        // 1) 让 WebView 走本地 ECH 代理
        val proxyConfig = ProxyConfig.Builder()
            .addProxy("${EchProxy.HOST}:${EchProxy.PORT_INT}")
            .build()
        ProxyController.getInstance()
            .setProxyOverride(proxyConfig, { /* applied */ }) { it.run() }

        // 2) 配置 WebView（严格按 Cloudflare 移动接入要求）
        webView = WebView(this)
        setContentView(webView)
        with(webView.settings) {
            javaScriptEnabled = true
            domStorageEnabled = true
            databaseEnabled = true
            loadWithOverviewMode = true
            useWideViewPort = true
            // 不调用 setUserAgentString：保持默认 UA，且全程一致
        }
        CookieManager.getInstance().apply {
            setAcceptCookie(true)
            setAcceptThirdPartyCookies(webView, true)
        }

        // 3) 监听挑战/登录结果
        webView.webViewClient = object : WebViewClient() {
            override fun onPageFinished(view: WebView?, url: String?) {
                val cookies = CookieManager.getInstance()
                    .getCookie("https://linux.do") ?: return
                AuthState.rawCookies = cookies
                val passed = cookies.contains("cf_clearance")
                val loggedIn = cookies.contains(Regex("(^|; )_t="))
                if (passed && (loggedIn || !requireLogin())) {
                    AuthState.passed = true
                    setResult(RESULT_OK)
                    finish()
                }
            }
        }

        // 直接加载首页：先过挑战，用户可在页面内点登录
        webView.loadUrl("https://linux.do/login")
    }

    private fun requireLogin() = intent.getBooleanExtra(EXTRA_REQUIRE_LOGIN, true)

    override fun onDestroy() {
        // 移除全局代理覆盖，避免影响其他 WebView
        ProxyController.getInstance().clearProxyOverride({ }, { it.run() })
        webView.destroy()
        super.onDestroy()
    }

    companion object {
        const val EXTRA_REQUIRE_LOGIN = "require_login"
    }
}

/** 登录/挑战态的进程内共享存储。生产环境可持久化到 EncryptedSharedPreferences。 */
object AuthState {
    @Volatile var rawCookies: String? = null
    @Volatile var passed: Boolean = false
}
