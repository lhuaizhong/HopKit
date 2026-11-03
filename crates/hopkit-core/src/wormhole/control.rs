use crate::ToolError;
use serde_json::{Value, json};
use std::sync::Mutex;
use std::time::Instant;
#[derive(Clone)]
pub struct Decision {
    pub action: String,
    pub path: Option<String>,
}
pub struct Control {
    pub(super) state: Mutex<State>,
    pub(super) tx: async_channel::Sender<Decision>,
    pub(super) rx: async_channel::Receiver<Decision>,
}
pub(super) struct State {
    pub public: Value,
    pub code: Option<String>,
    pub verifier: Option<String>,
    pub decision: Option<String>,
    pub processed: Option<String>,
    pub phase_at: Instant,
    pub progress_at: Instant,
    pub committing: bool,
}
impl Default for Control {
    fn default() -> Self {
        let (tx, rx) = async_channel::bounded(8);
        Self {
            tx,
            rx,
            state: Mutex::new(State {
                public: json!({"phase":"preparing","bytes":"0","total_bytes":"0","local_saved":false}),
                code: None,
                verifier: None,
                decision: None,
                processed: None,
                phase_at: Instant::now(),
                progress_at: Instant::now(),
                committing: false,
            }),
        }
    }
}
impl Control {
    pub fn set_task_id(&self, id: &str) {
        self.state.lock().unwrap().public["task_id"] = json!(id);
    }
    pub fn snapshot(&self) -> Value {
        self.state.lock().unwrap().public.clone()
    }
    pub fn private_state(&self) -> Value {
        let s = self.state.lock().unwrap();
        json!({"code":s.code,"verifier":s.verifier})
    }
    pub fn action(&self, id: &str, action: &str, path: Option<String>) -> Result<(), ToolError> {
        let mut s = self.state.lock().unwrap();
        if s.processed.as_deref() == Some(id) {
            return Err(ToolError::new(
                "ALREADY_PROCESSED",
                "Decision already processed / 已处理",
            ));
        }
        if s.public["deadline_ms"]
            .as_i64()
            .is_some_and(|d| chrono::Utc::now().timestamp_millis() >= d)
        {
            return Err(ToolError::new("TIMEOUT", "Decision expired / 确认已过期"));
        }
        if s.decision.as_deref() != Some(id)
            || !matches!(
                (s.public["phase"].as_str().unwrap_or(""), action),
                ("waiting_verification", "verify_ok" | "reject")
                    | ("waiting_acceptance", "accept" | "reject")
            )
        {
            return Err(ToolError::new(
                "INVALID_ARGUMENT",
                "Decision expired or wrong phase / 确认已过期或阶段不符",
            ));
        }
        self.tx
            .try_send(Decision {
                action: action.into(),
                path,
            })
            .map_err(|_| ToolError::new("RESOURCE_EXHAUSTED", "Decision queue full"))?;
        s.processed = Some(id.into());
        s.decision = None;
        Ok(())
    }
    pub(super) fn phase(&self, phase: &str) {
        let mut s = self.state.lock().unwrap();
        if phase == "closing" {
            s.public["transfer_phase"] = s.public["phase"].clone();
        }
        s.public["phase"] = json!(phase);
        s.phase_at = Instant::now();
        s.public["decision_id"] = Value::Null;
        s.decision = None;
    }
    pub(super) fn decision(&self, seconds: u64) {
        let mut s = self.state.lock().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        s.decision = Some(id.clone());
        s.public["decision_id"] = json!(id);
        s.public["deadline_ms"] = json!(
            chrono::Utc::now().timestamp_millis()
                + seconds.saturating_sub(s.phase_at.elapsed().as_secs()) as i64 * 1000
        );
    }
    pub(super) fn clear_private(&self) {
        let mut s = self.state.lock().unwrap();
        s.code = None;
        s.verifier = None;
        s.decision = None;
        s.public["decision_id"] = Value::Null;
    }
}
