// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Serializes hardware report generation onto the main runtime.
//!
//! The report reads sysfs through `cc_fs`, whose futures are not `Send`, so it
//! cannot run inside an Axum handler directly. Routing it through an actor is
//! the same arrangement every other repository-touching endpoint uses.

use anyhow::Result;
use log::error;
use moro_local::Scope;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::api::actor::{run_api_actor, ApiActor};
use crate::diagnostics::report;
use crate::diagnostics::support::HardwareSupportController;
use crate::rt;
use std::rc::Rc;
use std::time::Duration;

/// The report walks every hwmon directory itself, including devices the daemon
/// dropped, so it cannot take the per-device permits `HwmonRepo` holds. A
/// driver that never returns a read would otherwise wedge this actor for good,
/// taking `GET /detect` down with it, since both share the mailbox. Kept under
/// the API's own 30s timeout so the client sees the failure rather than a hang.
///
/// The last resort, not the working limit: `report::REPORT_READ_BUDGET`
/// bounds the reads themselves at 15s and returns a report that names the
/// device at fault. This fires only for a stall no `timeout` can interrupt,
/// such as a synchronous `read_dir` on a wedged filesystem.
const GENERATE_TIMEOUT: Duration = Duration::from_secs(20);

/// Returned instead of a partial report, which would read as the whole truth
/// about the machine when it is not.
const TIMED_OUT_REPORT: &str =
    "The hardware report timed out while reading sysfs. A driver is not answering; \
     check the daemon log for which device.";

struct HardwareReportActor {
    receiver: mpsc::Receiver<HardwareReportMessage>,
    hardware_support: Rc<HardwareSupportController>,
}

enum HardwareReportMessage {
    /// What the startup probe found, rather than a fresh probe. Startup is
    /// when module loading actually happens, so it is the only run that can
    /// answer "why is my chip not bound".
    RetainedDetection {
        respond_to: oneshot::Sender<Option<cc_detect::DetectionResults>>,
    },
    /// An explicit re-scan replaces what the retained results say, so the
    /// health snapshot stops contradicting a scan the user just ran.
    RefreshDetection {
        results: Box<cc_detect::DetectionResults>,
        respond_to: oneshot::Sender<()>,
    },
    Generate {
        full: bool,
        respond_to: oneshot::Sender<String>,
    },
}

impl HardwareReportActor {
    pub fn new(
        receiver: mpsc::Receiver<HardwareReportMessage>,
        hardware_support: Rc<HardwareSupportController>,
    ) -> Self {
        Self {
            receiver,
            hardware_support,
        }
    }
}

impl ApiActor<HardwareReportMessage> for HardwareReportActor {
    fn name(&self) -> &'static str {
        "HardwareReportActor"
    }

    fn receiver(&mut self) -> &mut mpsc::Receiver<HardwareReportMessage> {
        &mut self.receiver
    }

    async fn handle_message(&mut self, msg: HardwareReportMessage) {
        match msg {
            HardwareReportMessage::RetainedDetection { respond_to } => {
                let _ = respond_to.send(self.hardware_support.detection());
            }
            HardwareReportMessage::RefreshDetection {
                results,
                respond_to,
            } => {
                self.hardware_support.refresh_detection(*results);
                let _ = respond_to.send(());
            }
            HardwareReportMessage::Generate { full, respond_to } => {
                // Retained at startup, so this never re-enumerates USB devices
                // and still includes devices the user has disabled.
                let devices_other = self.hardware_support.device_summaries();
                // Recorded by the repositories as they dropped each device, so
                // the report says why a chip is missing from the app instead of
                // leaving it looking broken.
                let hidden = self.hardware_support.hidden_hardware();
                // Cloned rather than borrowed: the generation below awaits, and
                // a `RefCell` borrow must not be held across an await point.
                let detection = self.hardware_support.detection();
                // Read at startup by the repository and kept, so the compact
                // report describes what the daemon already knows rather than
                // walking sysfs again.
                let retained = self.hardware_support.hwmon_drivers();
                let report = rt::timeout(
                    GENERATE_TIMEOUT,
                    report::generate(full, detection.as_ref(), &devices_other, &hidden, &retained),
                )
                .await
                .unwrap_or_else(|_| {
                    error!("Timed out after {GENERATE_TIMEOUT:?} generating the hardware report");
                    TIMED_OUT_REPORT.to_string()
                });
                let _ = respond_to.send(report);
            }
        }
    }
}

/// Cloneable handle for generating the hardware report.
///
/// Capacity 1: the report is a human-triggered support action, not something
/// polled, so queueing more than one has no value and bounds the sysfs work a
/// burst of requests can start.
#[derive(Clone)]
pub struct HardwareReportHandle {
    sender: mpsc::Sender<HardwareReportMessage>,
}

impl HardwareReportHandle {
    pub fn new<'s>(
        hardware_support: Rc<HardwareSupportController>,
        cancel_token: CancellationToken,
        main_scope: &'s Scope<'s, 's, Result<()>>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(1);
        let actor = HardwareReportActor::new(receiver, hardware_support);
        main_scope.spawn(run_api_actor(actor, cancel_token));
        Self { sender }
    }

    /// `None` means no probe was made (disabled by config or environment),
    /// which is not the same as a probe that found nothing.
    pub async fn retained_detection(&self) -> Result<Option<cc_detect::DetectionResults>> {
        let (tx, rx) = oneshot::channel();
        let msg = HardwareReportMessage::RetainedDetection { respond_to: tx };
        let _ = self.sender.send(msg).await;
        Ok(rx.await?)
    }

    /// Hands a fresh scan back to the retained state. Called by `POST /detect`,
    /// which is the only request that can load modules after startup.
    pub async fn refresh_detection(&self, results: cc_detect::DetectionResults) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = HardwareReportMessage::RefreshDetection {
            results: Box::new(results),
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        Ok(rx.await?)
    }

    pub async fn generate(&self, full: bool) -> Result<String> {
        let (tx, rx) = oneshot::channel();
        let msg = HardwareReportMessage::Generate {
            full,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        Ok(rx.await?)
    }
}
