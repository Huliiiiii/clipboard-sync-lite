use anyhow::Result;
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, unbounded};
use std::thread;
use std::time::{Duration, Instant};
use x11_clipboard::Clipboard;
use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xfixes::{self, SelectionEventMask};
use x11rb::protocol::xproto::ConnectionExt;

fn main() -> Result<()> {
    env_logger::init();

    // 建立通道：主线程 -> 工作线程
    let (tx, rx) = unbounded::<()>();

    // --- 1. 启动工作线程 (Consumer) ---
    thread::spawn(move || {
        worker_loop(rx);
    });

    // --- 2. 主线程只负责监听信号 (Producer) ---
    // 注意：这里使用一个轻量级的连接，或者复用 x11rb 的裸连接
    // 为了简单，这里我们还是用 Clipboard::new，但只用它的连接来监听
    let watcher_ctx = Clipboard::new()?;
    let conn = &watcher_ctx.getter.connection;
    let screen = &conn.setup().roots[0];
    let root_window = screen.root;
    let clipboard_atom = watcher_ctx.getter.atoms.clipboard;

    xfixes::query_version(conn, 5, 0)?.reply()?;
    let mask = SelectionEventMask::SET_SELECTION_OWNER; // 我们只关心 Owner 变了
    xfixes::select_selection_input(conn, root_window, clipboard_atom, mask)?;
    conn.flush()?;

    log::info!("Listening for clipboard changes...");

    loop {
        // 这里永远不会因为 load_wait 而卡顿
        let event = conn.wait_for_event()?;

        if let Event::XfixesSelectionNotify(e) = event {
            if e.selection == clipboard_atom {
                // 收到信号，通知工作线程干活
                // 使用 try_send 避免通道积压
                let _ = tx.try_send(());
            }
        }
    }
}

fn worker_loop(rx: Receiver<()>) {
    // ... (连接建立代码不变) ...
    let worker_ctx = match Clipboard::new() {
        Ok(c) => c,
        Err(e) => {
            log::error!("{}", e);
            return;
        }
    };
    let mut last_text: Vec<u8> = Vec::new();
    // 【新增】记录最后一次写入 Wayland 的时间
    let mut last_write_time = Instant::now() - Duration::from_secs(10);
    loop {
        // ... (Receiver 接收代码不变) ...
        if let Err(_) = rx.recv() {
            break;
        }

        // 防抖逻辑 (不变)
        let debounce_time = Duration::from_millis(150);
        let mut skipped_signals = 0;
        loop {
            match rx.recv_timeout(debounce_time) {
                Ok(_) => {
                    skipped_signals += 1;
                    continue;
                }
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
        // 【关键修复 1：回声消除】
        // 如果现在距离上次写入 Wayland 不到 500ms，
        // 说明这个事件极有可能是 XWayland 响应我们的写入而发的。
        if last_write_time.elapsed() < Duration::from_millis(500) {
            log::debug!("Ignoring echo event (triggered by self).");
            continue;
        }
        // ... (Owner 检查代码不变，保留作为双重保险) ...
        log::debug!(
            "Silence detected (skipped {} signals), starting sync...",
            skipped_signals
        );

        // 传入 last_write_time 的可变引用，以便在成功写入后更新它
        if let Err(e) = x2w_safe(&worker_ctx, &mut last_text, &mut last_write_time) {
            log::error!("Sync failed: {}", e);
        }
    }
}
fn x2w_safe(xcb: &Clipboard, last_text: &mut Vec<u8>, last_write_time: &mut Instant) -> Result<()> {
    let prop_atom = xcb.setter.atoms.property;
    let at_cb = xcb.getter.atoms.clipboard;
    let utf8 = xcb.getter.atoms.utf8_string;
    let start = Instant::now();
    // 【关键修复 2：严格的超时设置】
    // 既然在独立线程，超时不再会导致主程序崩溃。
    // 如果 200ms 读不到，说明源程序有问题或者陷入了死锁，直接放弃。
    let result = xcb.load(
        at_cb,
        utf8,
        prop_atom,
        Duration::from_millis(200), // 设置 200ms 超时
    );
    let load_cost = start.elapsed().as_millis();
    if load_cost > 150 {
        log::warn!("Clipboard load took {} ms (Timeout logic hit?)", load_cost);
    }
    let data = match result {
        Ok(data) => data,
        Err(e) => {
            // 这里仅仅打印 debug，不要 error，因为超时在死锁情况下很正常
            log::debug!("Load aborted/timed out: {}", e);
            return Ok(());
        }
    };
    // 【关键修复 3：忽略空数据】
    if data.is_empty() {
        log::debug!("Loaded empty data, skipping copy.");
        return Ok(());
    }
    if data != *last_text {
        use wl_clipboard_rs::copy::{MimeType, Options, Source};
        let copy_opts = Options::new();

        match copy_opts.copy(Source::Bytes(data.clone().into()), MimeType::Autodetect) {
            Ok(_) => {
                log::info!("Synced X11 -> Wayland ({} bytes)", data.len());
                *last_text = data;

                // 【关键修复 1 配套】：更新写入时间
                *last_write_time = Instant::now();
            }
            Err(e) => log::error!("Wayland copy failed: {}", e),
        }
    } else {
        log::debug!("Data unchanged, skipped.");
    }
    Ok(())
}
