//! The cron event.
//!
//! `openworkers:worker/scheduled` carries only the trigger time, so
//! [`ScheduledEvent::cron`] has nothing to report and returns an empty
//! string.

use crate::rt;

#[derive(Debug, Clone)]
pub struct ScheduledEvent {
    cron: String,
    ty: String,
    schedule: f64,
}

impl ScheduledEvent {
    pub(crate) fn new(scheduled_time: u64) -> Self {
        ScheduledEvent {
            cron: String::new(),
            ty: "scheduled".to_string(),
            schedule: scheduled_time as f64,
        }
    }

    /// The cron expression that fired. The host does not send one.
    pub fn cron(&self) -> String {
        self.cron.clone()
    }

    pub fn ty(&self) -> String {
        self.ty.clone()
    }

    /// The trigger time, in milliseconds since the Unix epoch.
    pub fn schedule(&self) -> f64 {
        self.schedule
    }
}

/// The scheduled handler's context.
#[derive(Debug, Clone, Default)]
pub struct ScheduleContext;

impl ScheduleContext {
    /// Queues work to finish before the export returns.
    pub fn wait_until<T>(&self, handler: T)
    where
        T: std::future::Future<Output = ()> + 'static,
    {
        rt::spawn_local(handler);
    }
}
