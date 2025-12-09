use std::thread::sleep;
use std::time::{Duration, Instant};

use anyhow::Result;
use x11_clipboard::Clipboard;
use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xfixes::{self, SelectionEventMask};

struct Ctx {
    last_text: Vec<u8>,
}

fn main() -> Result<()> {
    env_logger::init();
    let xcb = Clipboard::new()?;
    let last_text = Vec::new();

    let mut ctx = Ctx { last_text };

    let conn = &xcb.getter.connection;
    let screen = &conn.setup().roots[0];
    let root_window = screen.root;

    let clipboard_atom = xcb.getter.atoms.clipboard;

    xfixes::query_version(conn, 5, 0)?.reply()?;

    let mask = SelectionEventMask::SET_SELECTION_OWNER
        | SelectionEventMask::SELECTION_WINDOW_DESTROY
        | SelectionEventMask::SELECTION_CLIENT_CLOSE;

    xfixes::select_selection_input(conn, root_window, clipboard_atom, mask)?;

    conn.flush()?;

    loop {
        let event = conn.wait_for_event()?;

        let start = std::time::Instant::now();
        // log::debug!("Get event {event:?}");
        if let Event::XfixesSelectionNotify(e) = event
            && e.selection == clipboard_atom
        {
            x2w(&xcb, &mut ctx.last_text)?;
        }

        let end = std::time::Instant::now();

        let duration = end - start;
        let duration = duration.as_millis();
        log::debug!("Duration: {duration} ms");
        sleep(Duration::from_millis(100));
    }
}

fn x2w(xcb: &Clipboard, last_text: &mut Vec<u8>) -> Result<()> {
    let prop_atom = xcb.setter.atoms.property;

    let at_cb = xcb.getter.atoms.clipboard;
    // let mut start = Instant::now();
    macro_rules! mesure {
        ($name:literal) => {
            // start = print_cost($name, start);
        };
    }
    let result = xcb.load_wait(
        at_cb,
        xcb.getter.atoms.utf8_string,
        prop_atom,
        // std::time::Duration::from_millis(800),
    );
    // let start =print_cost("load", start);
    mesure!("load");

    let data = match result {
        Ok(data) => data,
        Err(e) => {
            log::error!("Failed to load text: {e}");
            return Ok(());
        }
    };

    if data != *last_text {
        use wl_clipboard_rs::copy::{MimeType, Options, Source};
        let opts = Options::new();

        mesure!("copy start");
        match opts.copy(Source::Bytes(data.clone().into()), MimeType::Autodetect) {
            Ok(_) => {
                mesure!("copy end");
                *last_text = data
            }
            Err(e) => {
                log::error!("Failed to copy data: {e}")
            }
        }
    }

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
