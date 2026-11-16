//! JNI 入口：Kotlin 端通过 nativeStart/nativeStop 控制本地 ECH 代理。
//! 对应 Kotlin 类 com.linuxdo.news.core.EchProxy（包名改动需同步此处函数名）。

mod cert;
mod doh;
mod proxy;

use std::sync::Arc;

use jni::objects::JClass;
use jni::sys::{jbyteArray, jint};
use jni::JNIEnv;
use once_cell::sync::OnceCell;

use cert::LocalCa;
use doh::Doh;
use proxy::Proxy;

static RUNTIME: OnceCell<tokio::runtime::Runtime> = OnceCell::new();

/// 启动代理。参数 port 为本地监听端口；返回本地 CA 的 DER 字节（供 App 信任）。
#[no_mangle]
pub extern "system" fn Java_com_linuxdo_news_core_EchProxy_nativeStart(
    mut env: JNIEnv,
    _class: JClass,
    port: jint,
) -> jbyteArray {
    let result = std::panic::catch_unwind(|| start(port as u16));
    match result {
        Ok(Ok(ca_der)) => env
            .byte_array_from_slice(&ca_der)
            .map(|a| a.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        Ok(Err(e)) => {
            eprintln!("nativeStart 失败: {e}");
            std::ptr::null_mut()
        }
        Err(_) => std::ptr::null_mut(),
    }
}

fn start(port: u16) -> anyhow::Result<Vec<u8>> {
    if RUNTIME.get().is_none() {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()?;
        // 首次设置；重复启动时复用已有 runtime，避免新 runtime 在函数结束时被 drop
        let _ = RUNTIME.set(rt);
    }
    let handle = RUNTIME.get().expect("runtime 已初始化").handle().clone();

    let ca = Arc::new(LocalCa::new()?);
    let ca_der = ca.ca_der();
    let proxy = Arc::new(Proxy {
        ca,
        doh: Arc::new(Doh::new()),
    });
    handle.spawn(async move {
        if let Err(e) = proxy.run(port).await {
            eprintln!("代理运行结束: {e}");
        }
    });
    Ok(ca_der)
}

/// 停止代理并关闭后台运行时。
#[no_mangle]
pub extern "system" fn Java_com_linuxdo_news_core_EchProxy_nativeStop(
    _env: JNIEnv,
    _class: JClass,
) {
    if let Some(rt) = RUNTIME.get() {
        rt.shutdown_background();
    }
}
