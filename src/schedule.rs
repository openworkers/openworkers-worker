//! The cron event.
//!
//! Through the `openworkers:worker/task` export the host sends the cron
//! expression. A host that knows only `openworkers:worker/scheduled` sends the
//! trigger time alone, and [`ScheduledEvent::cron`] is then empty.

use crate::rt;

#[derive(Debug, Clone)]
pub struct ScheduledEvent {
    cron: String,
    ty: String,
    schedule: f64,
}

impl ScheduledEvent {
    pub(crate) fn new(scheduled_time: u64, cron: String) -> Self {
        ScheduledEvent {
            cron,
            ty: "scheduled".to_string(),
            schedule: scheduled_time as f64,
        }
    }

    /// The cron expression that fired. Empty when the host sends none.
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
