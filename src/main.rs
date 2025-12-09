use anyhow::Result;
use std::thread::sleep;
use std::time::{Duration, Instant};
use x11_clipboard::Clipboard;
use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xfixes::{self, SelectionEventMask};

fn main() -> Result<()> {
    env_logger::init();

    let xcb = Clipboard::new()?;
    let conn = &xcb.getter.connection;
    let screen = &conn.setup().roots[0];
    let root_window = screen.root;
    let clipboard_atom = xcb.getter.atoms.clipboard;

    xfixes::query_version(conn, 5, 0)?.reply()?;
    let mask = SelectionEventMask::SET_SELECTION_OWNER;
    xfixes::select_selection_input(conn, root_window, clipboard_atom, mask)?;
    conn.flush()?;

    let mut last_text: Vec<u8> = Vec::new();

    let mut last_write_time = Instant::now() - Duration::from_secs(10);

    log::info!("Listening for clipboard changes...");

    loop {
        let event = conn.wait_for_event()?;

        let Event::XfixesSelectionNotify(e) = event else {
            continue;
        };
        if e.selection != clipboard_atom {
            continue;
        }

        let debounce_time = Duration::from_millis(150);
        sleep(debounce_time);

        // Debounce
        while conn.poll_for_event()?.is_some() {}

        // Skip this event if it is triggered too frequently
        if last_write_time.elapsed() < Duration::from_millis(150) {
            log::debug!("Ignoring echo event (triggered by self).");
            continue;
        }

        if let Err(e) = x2w(&xcb, &mut last_text, &mut last_write_time) {
            log::error!("Sync failed: {}", e);
        }
    }
}

fn x2w(xcb: &Clipboard, last_text: &mut Vec<u8>, last_write_time: &mut Instant) -> Result<()> {
    let prop_atom = xcb.setter.atoms.property;
    let clipb_atom = xcb.getter.atoms.clipboard;

    let mut start = Instant::now();
    macro_rules! mesure {
        ($name:literal) => {
            start = print_cost($name, start);
        };
    }

    let result = xcb.load(
        clipb_atom,
        xcb.getter.atoms.utf8_string,
        prop_atom,
        Duration::from_millis(200),
    );
    mesure!("load");

    let data = match result {
        Ok(data) => data,
        Err(e) => {
            log::debug!("Load aborted/timed out: {}", e);
            return Ok(());
        }
    };

    if data.is_empty() {
        log::debug!("Loaded empty data, skipping copy.");
        return Ok(());
    }

    if data != *last_text {
        use wl_clipboard_rs::copy::{MimeType, Options, Source};
        let opts = Options::new();

        mesure!("copy start");
        match opts.copy(Source::Bytes(data.clone().into()), MimeType::Autodetect) {
            Ok(_) => {
                mesure!("copy end");
                log::info!("Synced X11 -> Wayland ({} bytes)", data.len());
                *last_text = data;
                *last_write_time = Instant::now();
            }
            Err(e) => log::error!("Wayland copy failed: {}", e),
        }
    } else {
        log::debug!("Data unchanged, skipped.");
    }

    let _ = start;

    Ok(())
}

#[allow(unused)]
fn print_cost(name: &str, start: Instant) -> Instant {
    let now = Instant::now();
    let dur = now - start;
    let ms = dur.as_millis();
    log::debug!("{name}: {ms} ms",);
    now
}
