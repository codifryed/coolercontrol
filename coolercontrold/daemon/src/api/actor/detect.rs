/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2025  Guy Boldon, Eren Simsek and contributors
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

use std::path::PathBuf;

use anyhow::Result;
use moro_local::Scope;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::api::actor::{run_api_actor, ApiActor};

struct DetectActor {
    receiver: mpsc::Receiver<DetectMessage>,
    /// Override path is held here so handlers carry no CoolerControl-specific paths.
    override_path: PathBuf,
}

enum DetectMessage {
    Run {
        load_modules: bool,
        respond_to: oneshot::Sender<Result<cc_detect::DetectionResults>>,
    },
}

impl DetectActor {
    pub fn new(receiver: mpsc::Receiver<DetectMessage>, override_path: PathBuf) -> Self {
        Self {
            receiver,
            override_path,
        }
    }
}

impl ApiActor<DetectMessage> for DetectActor {
    fn name(&self) -> &'static str {
        "DetectActor"
    }

    fn receiver(&mut self) -> &mut mpsc::Receiver<DetectMessage> {
        &mut self.receiver
    }

    async fn handle_message(&mut self, msg: DetectMessage) {
        match msg {
            DetectMessage::Run {
                load_modules,
                respond_to,
            } => {
                // run_detection probes I/O ports and optionally calls modprobe — both
                // are blocking operations. Offload to the blocking thread pool so the
                // async runtime is not stalled.
                let override_path = self.override_path.clone();
                let result = tokio::task::spawn_blocking(move || {
                    cc_detect::run_detection(load_modules, Some(&override_path))
                })
                .await
                .map_err(anyhow::Error::from);
                let _ = respond_to.send(result);
            }
        }
    }
}

/// Cloneable handle for serialized Super-I/O hardware detection.
///
/// The actor processes one detection at a time. This is a hardware-safety
/// requirement: concurrent I/O port probing of the same Super-I/O chip
/// would interleave config-mode entry/exit sequences and could corrupt chip state.
/// Channel capacity 1 allows one request to queue while another is running;
/// further requests await send, which is an async yield, not a thread block.
#[derive(Clone)]
pub struct DetectHandle {
    sender: mpsc::Sender<DetectMessage>,
}

impl DetectHandle {
    pub fn new<'s>(
        override_path: PathBuf,
        cancel_token: CancellationToken,
        main_scope: &'s Scope<'s, 's, Result<()>>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(1);
        let actor = DetectActor::new(receiver, override_path);
        main_scope.spawn(run_api_actor(actor, cancel_token));
        Self { sender }
    }

    pub async fn run(&self, load_modules: bool) -> Result<cc_detect::DetectionResults> {
        let (tx, rx) = oneshot::channel();
        let msg = DetectMessage::Run {
            load_modules,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }
}
