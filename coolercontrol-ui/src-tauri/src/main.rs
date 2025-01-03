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

use crate::commands::daemon_state::DaemonState;
use crate::commands::modes::ModesState;
use crate::commands::{daemon_state, modes, notifications};
use crate::plugins::port_finder::Port;
use crate::plugins::{
    port_finder, single_instance, wayland_ssd, wayland_top_level_icon, webkit_adjustments,
};
use std::env;
use std::sync::Arc;
use tauri::menu::{AboutMetadata, AboutMetadataBuilder};
use tauri::utils::config::FrontendDist;
use tauri::{App, AppHandle, Context};
use tauri_plugin_cli::CliExt;

mod commands;
mod plugins;
mod tray;

type UID = String;

const SYSTEM_TRAY_ID: &str = "coolercontrol-system-tray";
const MAIN_WINDOW_ID: &str = "main";

fn main() {
    single_instance::handle_startup();
    webkit_adjustments::handle_dma_rendering_for_nvidia_gpus();
    let Some(port) = port_finder::find_free_port() else {
        println!("ERROR: No free port on localhost found, exiting.");
        std::process::exit(1);
    };
    tauri::Builder::default()
        .manage(Arc::new(ModesState::default()))
        .manage(Arc::new(DaemonState::default()))
        .plugin(tauri_plugin_cli::init())
        .plugin(wayland_top_level_icon::init())
        .plugin(wayland_ssd::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_localhost::Builder::new(port).build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(single_instance::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::settings::start_in_tray_enable,
            commands::settings::start_in_tray_disable,
            commands::settings::get_start_in_tray,
            commands::settings::save_window_state,
            commands::settings::get_startup_delay,
            commands::settings::set_startup_delay,
            modes::set_modes,
            modes::set_active_modes,
            notifications::send_notification,
            daemon_state::connected_to_daemon,
            daemon_state::acknowledge_daemon_issues,
        ])
        .setup(move |app: &mut App| {
            handle_cli_arguments(app);
            tray::setup_system_tray(app)?;
            commands::settings::setup_config_store(app)?;
            Ok(())
        })
        .run(generate_localhost_context(port))
        .expect("error while running tauri application");
}

/// This function handles our special context localhost logic in the production build.
/// It not only allows us to use a localhost server for the Web UI assets, but also enables
/// http access to localhost in general, enabling us to access the locally running coolercontrold
/// daemon.
fn generate_localhost_context(port: Port) -> Context {
    let mut context = tauri::generate_context!();
    let url = format!("http://localhost:{port}").parse().unwrap();
    context.config_mut().build.frontend_dist = Some(FrontendDist::Url(url));
    context
}

fn handle_cli_arguments(app: &mut App) {
    if env::args_os().count() > 1 {
        let Ok(matches) = app.cli().matches() else {
            println!(
                "ERROR: Unknown argument. Use the --help option to list the available arguments."
            );
            std::process::exit(1);
        };
        if matches.args.contains_key("help") {
            println!(
                "
CoolerControl GUI Desktop Application v{}

OPTIONS:
-h, --help       Print help information (this)
-V, --version    Print version information",
                app.package_info().version
            );
            std::process::exit(0);
        } else if matches.args.contains_key("version")
            && matches.args.get("version").unwrap().value.is_null()
        {
            // value is Bool(false) if no argument is given...
            println!(
                "CoolerControl GUI Desktop Application v{}",
                app.package_info().version
            );
            std::process::exit(0);
        }
    }
}

fn create_metadata(app_handle: &AppHandle) -> AboutMetadata {
    let metadata = AboutMetadataBuilder::new()
        .name(Some("CoolerControl".to_string()))
        .icon(Some(
            app_handle
                .default_window_icon()
                .cloned()
                .expect("Failed to get default icon"),
        ))
        .authors(Some(vec![
            "Guy Boldon and project contributors https://gitlab.com/coolercontrol/coolercontrol/-/graphs/main?ref_type=heads".to_string(),
        ]))
        .license(Some(
            "Copyright (c) 2021-2024  Guy Boldon, Eren Simsek and contributors

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, either version 3 of the License, or
any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU General Public License for more details.

You should have received a copy of the GNU General Public License
along with this program.  If not, see <https://www.gnu.org/licenses/>."
                .to_string(),
        ))
        .version(app_handle.config().version.clone())
        .website(Some("https://gitlab.com/coolercontrol/coolercontrol"))
        .website_label(Some("GitLab Project Page"))
        .comments(Some(
            "Monitor and control your cooling devices".to_string(),
        ))
        .build();
    metadata
}
