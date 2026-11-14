//! Background check while the bypass is on: the same checks as the Checks
//! page, every 10 minutes and shortly after the network changes. A check that
//! passed earlier with the running strategy and fails now (twice, 30 s
//! apart, so a blip does not raise an alarm) is reported in the snapshot;
//! the interface offers to repair just that service. Nothing is changed
//! automatically.
use crate::{
    operation::OperationManager,
    selector::{Backend, Probes},
};
use anyhow::Result;
use std::{
    collections::BTreeSet,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex;
use zapret_core::Health;

pub const INTERVAL: Duration = Duration::from_secs(10 * 60);
/// How often the network is looked at.
pub const POLL: Duration = Duration::from_secs(30);
/// Second look at a failing check before reporting it.
pub const RECHECK: Duration = Duration::from_secs(30);

/// What the check remembers between runs.
#[derive(Default)]
pub struct Monitor {
    /// Strategy the remembered results belong to.
    strategy: String,
    /// Checks that passed with it at least once.
    known_good: BTreeSet<String>,
    last: Option<Instant>,
    network: Option<String>,
}

impl Monitor {
    /// A check is due after `INTERVAL` or when the network differs from the
    /// last look (a new Wi-Fi, a cable, a VPN adapter).
    pub fn due(&mut self, now: Instant, network: Option<String>) -> bool {
        let changed = network.is_some() && self.network.is_some() && network != self.network;
        self.network = network.or(self.network.take());
        changed
            || self
                .last
                .is_none_or(|last| now.duration_since(last) >= INTERVAL)
    }

    /// Checks that worked with `strategy` before and are missing from `passed`.
    fn compare(&mut self, strategy: &str, passed: &BTreeSet<String>) -> BTreeSet<String> {
        if strategy != self.strategy {
            self.strategy = strategy.to_owned();
            self.known_good.clear();
        }
        let broken = self.known_good.difference(passed).cloned().collect();
        self.known_good.extend(passed.iter().cloned());
        broken
    }

    fn forget(&mut self) {
        self.strategy.clear();
        self.known_good.clear();
    }
}

fn passed(results: &[zapret_core::ProbeResult]) -> BTreeSet<String> {
    results
        .iter()
        .filter(|r| r.ok)
        .map(|r| r.id.clone())
        .collect()
}

fn idle(state: &OperationManager) -> bool {
    state.gate.try_lock().is_ok()
}

/// One background check. Skipped while an operation (selection, a check the
/// user started, a bundle switch) runs; results seen during one are dropped.
pub async fn tick<B: Backend, P: Probes>(
    monitor: &mut Monitor,
    state: &OperationManager,
    backend: &Arc<Mutex<B>>,
    probes: &P,
    recheck: Duration,
) -> Result<()> {
    monitor.last = Some(Instant::now());
    if !idle(state) {
        return Ok(());
    }
    // A finished selection or repair resets the published result.
    if state.health.lock().await.checked_at == 0 {
        monitor.forget();
    }
    let status = backend.lock().await.status()?;
    if !status.running {
        monitor.forget();
        *state.health.lock().await = Health::default();
        return Ok(());
    }
    let first = probes.run().await;
    let mut broken = monitor.compare(&status.strategy, &passed(&first));
    if !broken.is_empty() && !recheck.is_zero() {
        tokio::time::sleep(recheck).await;
        let again = passed(&probes.run().await);
        broken.retain(|c| !again.contains(c));
        monitor.compare(&status.strategy, &again);
    }
    let still = backend.lock().await.status()?;
    if !idle(state) || !still.running || still.strategy != status.strategy {
        return Ok(());
    }
    *state.health.lock().await = Health {
        checked_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(1, |d| d.as_secs().max(1)),
        broken: broken.into_iter().collect(),
    };
    Ok(())
}

/// Runs until `shutdown` changes.
pub async fn run<B: Backend + 'static, P: Probes + 'static>(
    state: Arc<OperationManager>,
    backend: Arc<Mutex<B>>,
    // Checks of the active bundle, read when a check starts.
    probes: impl Fn(&mut B) -> P + Send + 'static,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let mut monitor = Monitor::default();
    // The first look only learns the network; the first check runs after
    // the engine had time to start.
    monitor.due(Instant::now(), crate::secure_dns::network_signature());
    // Instant cannot go below boot time, and the service starts at boot.
    monitor.last = Instant::now().checked_sub(INTERVAL - Duration::from_secs(60));
    loop {
        tokio::select! {
            _ = tokio::time::sleep(POLL) => {}
            _ = shutdown.changed() => return,
        }
        if monitor.due(Instant::now(), crate::secure_dns::network_signature()) {
            let probes = probes(&mut *backend.lock().await);
            let _ = tick(&mut monitor, &state, &backend, &probes, RECHECK).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(ids: &[&str]) -> BTreeSet<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn due_on_interval_and_network_change() {
        let mut m = Monitor::default();
        let start = Instant::now();
        assert!(m.due(start, Some("home".into())), "first check");
        m.last = Some(start);
        assert!(!m.due(start + POLL, Some("home".into())));
        assert!(!m.due(start + POLL, None), "unknown network is no change");
        assert!(m.due(start + POLL, Some("cafe".into())));
        m.last = Some(start + POLL);
        assert!(m.due(start + POLL + INTERVAL, Some("cafe".into())));
    }

    #[test]
    fn only_checks_that_worked_before_count_as_broken() {
        let mut m = Monitor::default();
        assert!(m.compare("a", &set(&["internet", "youtube"])).is_empty());
        // Discord never worked with "a": not broken, YouTube stopped: broken.
        assert_eq!(m.compare("a", &set(&["internet"])), set(&["youtube"]));
        assert!(m.compare("a", &set(&["internet", "youtube"])).is_empty());
        // Another strategy starts from scratch.
        assert!(m.compare("b", &set(&["internet"])).is_empty());
    }
}
