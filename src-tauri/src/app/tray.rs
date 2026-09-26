//! Tray icon: turn the camera on/off, open the window, quit.

use std::sync::Arc;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

use super::controller::{Controller, Snapshot};
use super::i18n;

const TRAY_ID: &str = "main";

/// Kept to change the toggle's text when the camera turns on or off.
struct ToggleItem(MenuItem<Wry>);

pub fn create(app: &AppHandle, snap: &Snapshot) -> tauri::Result<()> {
    let t = i18n::tray(&snap.language);
    let toggle = MenuItem::with_id(app, "toggle", if snap.camera_on { t.off } else { t.on }, true, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", t.open, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", t.quit, true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&toggle, &open, &PredefinedMenuItem::separator(app)?, &quit])?;
    app.manage(ToggleItem(toggle));

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(app.default_window_icon().cloned().expect("app icon"))
        .tooltip(t.tooltip_off)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "toggle" => {
                let c = app.state::<Arc<Controller>>().inner().clone();
                std::thread::spawn(move || c.set_camera_on(!c.is_camera_on()));
            }
            "open" => super::show_window(app),
            "quit" => super::quit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                super::show_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

pub fn update(app: &AppHandle, snap: &Snapshot) {
    let t = i18n::tray(&snap.language);
    if let Some(item) = app.try_state::<ToggleItem>() {
        let _ = item.0.set_text(if snap.camera_on { t.off } else { t.on });
    }
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_tooltip(Some(if snap.camera_on { t.tooltip_on } else { t.tooltip_off }));
    }
}
