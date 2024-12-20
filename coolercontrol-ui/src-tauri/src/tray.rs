/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2024  Guy Boldon, Eren Simsek and contributors
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 ******************************************************************************/

use crate::commands::modes::{EventPayload, ModeTauri, ModesState};
use crate::{create_metadata, MAIN_WINDOW_ID, SYSTEM_TRAY_ID, UID};
use std::error::Error;
use std::sync::MutexGuard;
use tauri::menu::{
    CheckMenuItemBuilder, IconMenuItemBuilder, MenuBuilder, MenuEvent, MenuItemBuilder,
    SubmenuBuilder,
};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconEvent};
use tauri::{App, AppHandle, Emitter, Manager, Wry};

pub fn setup_system_tray(app: &mut App) -> Result<(), Box<dyn Error>> {
    let tray_menu_builder = create_starting_tray_menu(app.handle());
    let tray_menu = add_final_tray_menu_items(app.handle(), tray_menu_builder).build()?;
    // The TrayIcon is created by tauri with the icon already:
    let tray_icon = app
        .handle()
        .tray_by_id(SYSTEM_TRAY_ID)
        .expect("Failed to get tray icon");
    tray_icon.set_menu(Some(tray_menu))?;
    tray_icon.on_tray_icon_event(handle_sys_tray_event);
    tray_icon.on_menu_event(handle_tray_menu_event);
    Ok(())
}

fn create_starting_tray_menu(app_handle: &AppHandle) -> MenuBuilder<Wry, AppHandle<Wry>> {
    let tray_menu_item_cc = IconMenuItemBuilder::with_id("cc", "CoolerControl")
        // Using an icon also creates an icon column in the tray menu for all menu items.
        // Perhaps that will change in the future and we can use both checked and Icons.
        // .icon(Image::from_path("icons/icon.png").unwrap()))
        .icon(
            app_handle
                .default_window_icon()
                .cloned()
                .expect("Failed to get default icon"),
        )
        .enabled(false)
        .build(app_handle)
        .expect("Failed to build menu item");
    MenuBuilder::new(app_handle)
        .item(&tray_menu_item_cc)
        .separator()
}

pub fn recreate_mode_menu_items(
    app_handle: &AppHandle,
    active_modes_lock: &MutexGuard<Vec<UID>>,
    modes_state_lock: &MutexGuard<Vec<ModeTauri>>,
) {
    let modes_submenu_builder = SubmenuBuilder::with_id(app_handle, "modes", "Modes");
    let modes_submenu = if modes_state_lock.len() > 0 {
        modes_state_lock
            .iter()
            .fold(modes_submenu_builder, |menu, mode| {
                let mode_is_active = active_modes_lock.contains(&mode.uid);
                let mode_menu_item =
                    CheckMenuItemBuilder::with_id(mode.uid.clone(), mode.name.clone())
                        .checked(mode_is_active)
                        .build(app_handle)
                        .expect("Failed to build menu item");
                menu.item(&mode_menu_item)
            })
    } else {
        modes_submenu_builder.enabled(false)
    }
    .build()
    .expect("Failed to build submenu");
    let menu_with_modes = create_starting_tray_menu(app_handle).item(&modes_submenu);
    let tray_menu = add_final_tray_menu_items(app_handle, menu_with_modes)
        .build()
        .expect("Failed to build tray menu with modes");
    app_handle
        .tray_by_id(SYSTEM_TRAY_ID)
        .expect("Failed to get tray icon")
        .set_menu(Some(tray_menu))
        .expect("Failed to set new tray menu");
}

fn add_final_tray_menu_items<'m>(
    app_handle: &AppHandle,
    tray_menu: MenuBuilder<'m, Wry, AppHandle<Wry>>,
) -> MenuBuilder<'m, Wry, AppHandle<Wry>> {
    let tray_menu_item_show = MenuItemBuilder::with_id("show", "Show/Hide")
        .build(app_handle)
        .expect("Failed to build menu item");
    let tray_menu_item_quit = MenuItemBuilder::with_id("quit", "Quit")
        .build(app_handle)
        .expect("Failed to build menu item");
    tray_menu
        .separator()
        .about(Some(create_metadata(app_handle)))
        .item(&tray_menu_item_show)
        .item(&tray_menu_item_quit)
}

/// These events are not currently supported on Linux, but will leave for possible future support:
fn handle_sys_tray_event(tray_icon: &TrayIcon, tray_icon_event: TrayIconEvent) {
    if let TrayIconEvent::Click {
        button: MouseButton::Left,
        button_state: MouseButtonState::Up,
        ..
    } = tray_icon_event
    {
        let Some(window) = tray_icon.app_handle().get_webview_window(MAIN_WINDOW_ID) else {
            return;
        };
        window.show().expect("Failed to show window");
        window.set_focus().expect("Failed to set focus");
    }
}

fn handle_tray_menu_event(app: &AppHandle, event: MenuEvent) {
    match event.id().as_ref() {
        "quit" => {
            app.exit(0);
        }
        "show" => {
            let Some(window) = app.get_webview_window(MAIN_WINDOW_ID) else {
                return;
            };
            if window.is_visible().unwrap() {
                // is_minimized() doesn't seem to work on Linux atm
                if window.is_minimized().unwrap() {
                    window.unminimize().unwrap();
                    window.hide().unwrap();
                    window.show().unwrap();
                } else {
                    window.hide().unwrap();
                }
            } else {
                window.show().unwrap();
            }
        }
        id => {
            if id.len() == 36 {
                // Mode UUID
                // println!("System Tray Menu Item Click with Mode ID: {}", id);
                let modes_state = app.state::<ModesState>();
                let active_modes_lock = modes_state
                    .active_modes
                    .lock()
                    .expect("Active Mode State is poisoned");
                for active_mode_uid in active_modes_lock.iter() {
                    if active_mode_uid != id {
                        continue;
                    }
                    // this sets the menu item back to selected (since it's deselected it)
                    let modes_state_lock =
                        modes_state.modes.lock().expect("Modes State is poisoned");
                    recreate_mode_menu_items(
                        app.app_handle(),
                        &active_modes_lock,
                        &modes_state_lock,
                    );
                    return;
                }
                app.emit(
                    "mode-activated",
                    EventPayload {
                        active_mode_uid: id.to_owned(),
                    },
                )
                .unwrap();
            }
        }
    }
}
