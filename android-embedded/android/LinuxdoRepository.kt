package com.linuxdo.news.core

import android.content.Context
import android.webkit.WebSettings
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import okhttp3.Request
import org.json.JSONObject
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.util.concurrent.TimeUnit

/** 触发 Cloudflare 挑战、需要重新拉起 WebView 的信号。 */
class ChallengeRequiredException : Exception("Cloudflare 挑战未通过，请重新打开 WebView 认证")

data class Topic(
    val id: Long,
    val title: String,
    val slug: String,
    val createdAt: Instant,
    val bumpedAt: Instant,
    val postsCount: Int,
    val likeCount: Int,
) {
    val url: String get() = "https://linux.do/t/topic/$id"
}

/**
 * 统一数据层：OkHttp 走本地 ECH 代理，复用 WebView 拿到的 cookie 与 UA。
 * 遇到挑战页即抛 [ChallengeRequiredException]，由 UI 层重新启动 WebViewAuthActivity 后重试。
 */
class LinuxdoRepository(context: Context) {

    private val userAgent = WebSettings.getDefaultUserAgent(context)

    private val client = OkHttpClient.Builder()
        .proxy(EchProxy.proxy())
        .connectTimeout(8, TimeUnit.SECONDS)
        .readTimeout(10, TimeUnit.SECONDS)
        .addInterceptor { chain ->
            val req = chain.request().newBuilder()
                .header("Cookie", AuthState.rawCookies.orEmpty())
                .header("Accept", "application/json")
                .header("User-Agent", userAgent) // 与 WebView 全程一致
                .build()
            chain.proceed(req)
        }
        .build()

    /** 拉取本地当天零点之后新发布的全部话题，per_page=100 翻页，遇零点边界停止。 */
    suspend fun fetchToday(): List<Topic> = withContext(Dispatchers.IO) {
        val cutoff = LocalDate.now()
            .atStartOfDay(ZoneId.systemDefault())
            .toInstant()
        val result = LinkedHashMap<Long, Topic>()

        for (page in 1..30) {
            val json = getLatestPage(page)
            val topics = parseTopics(json)
            if (topics.isEmpty()) break

            topics.filter { it.createdAt >= cutoff }.forEach { result[it.id] = it }
            // created_at 必不晚于 bumped_at，越过零点即可保证当天无遗漏
            if (topics.any { it.bumpedAt < cutoff }) break
        }
        result.values.sortedByDescending { it.createdAt }
    }

    private fun getLatestPage(page: Int): JSONObject {
        val url = "https://linux.do/latest.json?page=$page&per_page=100"
        val request = Request.Builder().url(url).build()
        client.newCall(request).execute().use { resp ->
            val text = resp.body?.string().orEmpty()
            val isChallenge =
                !resp.isSuccessful ||
                text.contains("Just a moment", true) ||
                text.contains("cf-mitigated", true) ||
                text.contains("challenge-platform", true)
            if (isChallenge) throw ChallengeRequiredException()
            return JSONObject(text)
        }
    }

    private fun parseTopics(json: JSONObject): List<Topic> {
        val arr = json.optJSONObject("topic_list")?.optJSONArray("topics") ?: return emptyList()
        return (0 until arr.length()).map { i ->
            val o = arr.getJSONObject(i)
            Topic(
                id = o.getLong("id"),
                title = o.getString("title"),
                slug = o.optString("slug"),
                createdAt = Instant.parse(o.getString("created_at")),
                bumpedAt = Instant.parse(o.getString("bumped_at")),
                postsCount = o.optInt("posts_count"),
                likeCount = o.optInt("like_count"),
            )
        }
    }
}
