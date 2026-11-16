package com.linuxdo.news.core

import android.content.Context
import java.io.File
import java.net.InetSocketAddress
import java.net.Proxy

/**
 * 本地 ECH 代理的单例入口。
 * 对应 Rust 库 liblinuxdo_ech_core.so 中的 JNI 函数。
 */
object EchProxy {
    private const val LIB_NAME = "linuxdo_ech_core"
    private const val PORT = 17840

    @Volatile
    private var loaded = false

    @Volatile
    private var started = false

    /** 启动后返回的本地 CA（DER）。生产环境建议固定 CA，见 README 第 4 节。 */
    lateinit var caDerFile: File
        private set

    @Synchronized
    fun start(context: Context): File {
        if (!loaded) {
            System.loadLibrary(LIB_NAME)
            loaded = true
        }
        if (started) return caDerFile

        val der = nativeStart(PORT)
        check(der.isNotEmpty()) { "ECH 核心启动失败，返回空 CA" }
        caDerFile = File(context.filesDir, "ech_local_ca.der").apply { writeBytes(der) }
        started = true
        return caDerFile
    }

    @Synchronized
    fun stop() {
        if (started && loaded) {
            runCatching { nativeStop() }
            started = false
        }
    }

    /** 供 OkHttp 使用的本地代理地址。 */
    fun proxy(): Proxy =
        Proxy(Proxy.Type.HTTP, InetSocketAddress("127.0.0.1", PORT))

    /** 供 androidx.webkit ProxyController 使用的 host:port。 */
    const val HOST = "127.0.0.1"
    const val PORT_INT = PORT

    private external fun nativeStart(port: Int): ByteArray
    private external fun nativeStop()
}
