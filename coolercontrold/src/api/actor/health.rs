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
 */

use crate::api::actor::{run_api_actor, ApiActor};
use crate::api::base::{HealthCheck, HealthDetails, SystemDetails};
use crate::{Repos, VERSION};
use anyhow::Result;
use chrono::Local;
use moro_local::Scope;
use nix::unistd::Pid;
use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

struct HealthActor {
    receiver: mpsc::Receiver<HealthMessage>,
    repos: Repos,
}

enum HealthMessage {
    Check {
        warnings: usize,
        errors: usize,
        respond_to: oneshot::Sender<Result<HealthCheck>>,
    },
}

impl HealthActor {
    pub fn new(receiver: mpsc::Receiver<HealthMessage>, repos: Repos) -> Self {
        Self { receiver, repos }
    }
}

impl ApiActor<HealthMessage> for HealthActor {
    fn name(&self) -> &str {
        "HealthActor"
    }

    fn receiver(&mut self) -> &mut mpsc::Receiver<HealthMessage> {
        &mut self.receiver
    }

    #[allow(clippy::cast_sign_loss, clippy::cast_precision_loss)]
    async fn handle_message(&mut self, msg: HealthMessage) {
        match msg {
            HealthMessage::Check {
                warnings,
                errors,
                respond_to,
            } => {
                let response = async {
                    let sys_info = sysinfo::System::new_with_specifics(
                        sysinfo::RefreshKind::nothing()
                            .with_processes(sysinfo::ProcessRefreshKind::nothing().with_memory()),
                    );
                    let pid = sysinfo::Pid::from_u32(Pid::this().as_raw() as u32);
                    let daemon_process = sys_info.process(pid);
                    let status =
                        daemon_process.map_or_else(|| "UP".to_string(), |p| p.status().to_string());
                    let uptime = daemon_process.map_or_else(
                        || "00:00:00".to_string(),
                        |p| {
                            let total_seconds = p.run_time();
                            let hours = total_seconds / 3600;
                            let minutes = (total_seconds % 3600) / 60;
                            let seconds = total_seconds % 60;
                            format!("{hours:02}:{minutes:02}:{seconds:02}")
                        },
                    );
                    let memory_mb = daemon_process.map_or(0., |p| {
                        // bytes to MB rounded to double precision.
                        ((p.memory() as f64 / 1024. / 1024.) * 100.).round() / 100.
                    });
                    let version = VERSION.unwrap_or_default().to_string();
                    let liquidctl_connected = self.repos.liquidctl.is_some();
                    let system_name = sysinfo::System::host_name().unwrap_or_default();
                    HealthCheck {
                        status,
                        description: "Health check for CoolerControl Daemon".to_string(),
                        current_timestamp: Local::now(),
                        details: HealthDetails {
                            uptime,
                            version,
                            pid: pid.as_u32(),
                            memory_mb,
                            warnings,
                            errors,
                            liquidctl_connected,
                        },
                        system: SystemDetails { name: system_name },
                        links: HashMap::from([
                            (
                                "repository".to_string(),
                                "https://gitlab.com/coolercontrol/coolercontrol".to_string(),
                            ),
                            (
                                "wiki".to_string(),
                                "https://gitlab.com/coolercontrol/coolercontrol/-/wikis/home"
                                    .to_string(),
                            ),
                        ]),
                    }
                }
                .await;
                let _ = respond_to.send(Ok(response));
            }
        }
    }
}

#[derive(Clone)]
pub struct HealthHandle {
    sender: mpsc::Sender<HealthMessage>,
}

impl HealthHandle {
    pub fn new<'s>(
        repos: Repos,
        cancel_token: CancellationToken,
        main_scope: &'s Scope<'s, 's, Result<()>>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(10);
        let actor = HealthActor::new(receiver, repos);
        main_scope.spawn(run_api_actor(actor, cancel_token));
        Self { sender }
    }

    pub async fn check(&self, warnings: usize, errors: usize) -> Result<HealthCheck> {
        let (tx, rx) = oneshot::channel();
        let msg = HealthMessage::Check {
            warnings,
            errors,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }
}
