package com.linuxdo.news

import android.app.Application
import android.content.Intent
import com.linuxdo.news.core.EchProxy
import com.linuxdo.news.core.WebViewAuthActivity

/**
 * Application 启动时拉起本地 ECH 代理。
 * 首次（或 cookie 失效）由业务界面根据 ChallengeRequiredException 启动 WebViewAuthActivity。
 */
class NewsApp : Application() {
    override fun onCreate() {
        super.onCreate()
        // 代理在后台线程启动，避免阻塞主线程；这里仅作演示，实际可放协程。
        Thread {
            runCatching { EchProxy.start(this) }
                .onFailure { it.printStackTrace() }
        }.start()
    }

    /** 业务层调用：打开 WebView 完成挑战/登录。 */
    fun authIntent(requireLogin: Boolean = true): Intent =
        Intent(this, WebViewAuthActivity::class.java)
            .putExtra(WebViewAuthActivity.EXTRA_REQUIRE_LOGIN, requireLogin)
}
