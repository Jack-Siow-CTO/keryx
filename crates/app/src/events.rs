use keryx_domain::{RunEvent, RunEventKind, RunId};
use std::collections::HashMap;
use std::sync::Mutex;
use tokio::sync::broadcast;

const BROADCAST_CAPACITY: usize = 256;

struct RunChannel {
    next_seq: u64,
    buffer: Vec<RunEvent>,
    tx: broadcast::Sender<RunEvent>,
}

/// In-process Run event log + live fan-out for SSE subscribers.
#[derive(Default)]
pub struct RunEventHub {
    channels: Mutex<HashMap<RunId, RunChannel>>,
}

impl RunEventHub {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn channel_locked(channels: &mut HashMap<RunId, RunChannel>, run_id: RunId) -> &mut RunChannel {
        channels.entry(run_id).or_insert_with(|| {
            let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);
            RunChannel {
                next_seq: 1,
                buffer: Vec::new(),
                tx,
            }
        })
    }

    /// Append and broadcast a Run event. Returns the sequenced event.
    pub fn publish(&self, run_id: RunId, kind: RunEventKind) -> Result<RunEvent, String> {
        let mut channels = self.channels.lock().map_err(|e| e.to_string())?;
        let channel = Self::channel_locked(&mut channels, run_id);
        let event = RunEvent {
            run_id,
            seq: channel.next_seq,
            kind,
        };
        channel.next_seq += 1;
        channel.buffer.push(event.clone());
        let _ = channel.tx.send(event.clone());
        Ok(event)
    }

    /// Replay buffered events and subscribe to live updates.
    pub fn subscribe(
        &self,
        run_id: RunId,
    ) -> Result<(Vec<RunEvent>, broadcast::Receiver<RunEvent>), String> {
        let mut channels = self.channels.lock().map_err(|e| e.to_string())?;
        let channel = Self::channel_locked(&mut channels, run_id);
        Ok((channel.buffer.clone(), channel.tx.subscribe()))
    }

    /// Buffered events only (for tests / debug).
    pub fn buffered(&self, run_id: RunId) -> Result<Vec<RunEvent>, String> {
        let channels = self.channels.lock().map_err(|e| e.to_string())?;
        Ok(channels
            .get(&run_id)
            .map(|c| c.buffer.clone())
            .unwrap_or_default())
    }
}
