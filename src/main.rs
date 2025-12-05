use anyhow::Result;
use x11_clipboard::Clipboard;
use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xfixes::{self, SelectionEventMask};

fn main() -> Result<()> {
    let xcb = Clipboard::new()?;
    let mut last_text = Vec::new();

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
        log::debug!("Get event {event:?}");
        if let Event::XfixesSelectionNotify(e) = event
            && e.selection == clipboard_atom
        {
            x2w(&xcb, &mut last_text);
        }
    }
}

fn x2w(xcb: &Clipboard, last_text: &mut Vec<u8>) {
    let result = xcb.load_wait(
        xcb.getter.atoms.clipboard,
        xcb.getter.atoms.utf8_string,
        xcb.getter.atoms.property,
    );

    let data = match result {
        Ok(data) => data,
        Err(e) => {
            log::error!("Failed to load clipboard: {e}");
            return;
        }
    };

    if data != *last_text {
        use wl_clipboard_rs::copy::{MimeType, Options, Source};
        let opts = Options::new();

        match opts.copy(Source::Bytes(data.clone().into()), MimeType::Autodetect) {
            Ok(_) => *last_text = data,
            Err(e) => {
                log::error!("Failed to copy data: {e}")
            }
        }
    }
}
