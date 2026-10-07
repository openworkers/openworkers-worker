//! The task event. `#[event(task)]` gets every task the platform runs, cron
//! included, with its payload and its source. Cloudflare's crate has no
//! counterpart: this is OpenWorkers' own.

use serde::de::DeserializeOwned;

use crate::schedule::ScheduleContext;
use crate::wit_task::exports::openworkers::worker::task as wit;
use crate::Result;

/// The `#[event(task)]` handler's context.
pub type TaskContext = ScheduleContext;

/// What started a task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskSource {
    /// A cron schedule fired. `time` is in milliseconds since the Unix epoch.
    Schedule { time: u64, cron: Option<String> },
    /// Another task created this one.
    Chained { parent_task_id: String },
    /// A worker created this one.
    Worker { worker_id: String },
    /// Run by hand, from "cli", "dashboard" or "api".
    Invoke { origin: Option<String> },
}

#[derive(Debug, Clone)]
pub struct TaskEvent {
    id: String,
    attempt: u32,
    payload: Option<serde_json::Value>,
    source: Option<TaskSource>,
}

impl TaskEvent {
    pub(crate) fn from_wit(event: wit::TaskEvent) -> Result<Self> {
        let payload = match event.payload {
            Some(json) => Some(serde_json::from_str(&json)?),
            None => None,
        };

        Ok(TaskEvent {
            id: event.task_id,
            attempt: event.attempt,
            payload,
            source: event.source.map(TaskSource::from_wit),
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    /// 1 on the first run.
    pub fn attempt(&self) -> u32 {
        self.attempt
    }

    /// The payload, read as `T`. `None` when the task has none.
    pub fn payload<T: DeserializeOwned>(&self) -> Result<Option<T>> {
        match &self.payload {
            Some(value) => Ok(Some(serde_json::from_value(value.clone())?)),
            None => Ok(None),
        }
    }

    pub fn source(&self) -> Option<&TaskSource> {
        self.source.as_ref()
    }
}

impl TaskSource {
    fn from_wit(source: wit::TaskSource) -> Self {
        match source {
            wit::TaskSource::Schedule(schedule) => TaskSource::Schedule {
                time: schedule.time,
                cron: schedule.cron,
            },
            wit::TaskSource::Chained(parent_task_id) => TaskSource::Chained { parent_task_id },
            wit::TaskSource::Worker(worker_id) => TaskSource::Worker { worker_id },
            wit::TaskSource::Invoke(origin) => TaskSource::Invoke { origin },
        }
    }
}
