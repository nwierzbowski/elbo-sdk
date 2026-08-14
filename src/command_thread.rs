use iceoryx2::prelude::*;
use pivot_com_types::{EngineCommand, EngineResponse};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;
use crossbeam::channel;

const COMMAND_SERVICE_NAME: &str = "PivotEngine/CommandService";
const COMMAND_EVENT_SERVICE_NAME: &str = "PivotEngine/CommandEvents";

pub struct CommandWork {
    pub cmd: EngineCommand,
    // A one-shot channel to send the response back to the caller
    pub response_tx: channel::Sender<Result<EngineResponse, String>>,
}

pub fn spawn_command_thread(
    node: Arc<Node<ipc::Service>>,
    command_rx: channel::Receiver<CommandWork>,
    shutdown: Arc<AtomicBool>,
) -> std::thread::JoinHandle<()> {
    thread::spawn(move || {

        let (service, notifier) = loop {
            let cmd_service = node
                .service_builder(&COMMAND_SERVICE_NAME.try_into().unwrap())
                .request_response::<EngineCommand, EngineResponse>()
                .open();

            let cmd_event_service = node
                .service_builder(&COMMAND_EVENT_SERVICE_NAME.try_into().unwrap())
                .event()
                .open();

            match (cmd_service, cmd_event_service) {
                (Ok(s), Ok(n)) => break (s, n),
                _ => {
                    // Engine isn't fully ready yet, or services aren't registered.
                    // Sleep for a bit and try again.
                    thread::sleep(Duration::from_millis(500));
                    println!("Waiting for Engine command services to appear...");
                }
            }
        };

        println!("Command service loop active.");

        let iox_client = service.client_builder().create().unwrap();
        let cmd_notifier = notifier
            .notifier_builder()
            .create()
            .expect("Failed to create Notifier");

        while !shutdown.load(Ordering::Relaxed) {
            while let Ok(work) = command_rx.recv_timeout(Duration::from_millis(200)) {
                let result = (|| -> Result<EngineResponse, String> {
                    let request = iox_client
                        .loan_uninit()
                        .map_err(|e| format!("SHM loan failed: {}", e))?;
                    let pending = request
                        .write_payload(work.cmd)
                        .send()
                        .map_err(|e| format!("Send failed: {}", e))?;
                    // Notify the engine that a new command is available
                    cmd_notifier
                        .notify()
                        .map_err(|e| format!("Notifier failed: {}", e))?;
                    let res = pending
                        .receive()
                        .map_err(|e| format!("Receive failed: {}", e))?
                        .ok_or_else(|| "Engine closed the request without a response".to_string())?;
                    Ok(*res.payload())
                })();

                let _ = work.response_tx.send(result);
            }
        }
        println!("Command service loop exiting.");
    })
}
