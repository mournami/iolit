//! Selection moved from the Tauri client; rounds and rollback retain their order.
use crate::operation::OperationManager;
use anyhow::Result;
use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    sync::{Arc, atomic::Ordering},
};
use tokio::{sync::Mutex, time::Duration};
use zapret_core::{ProbeResult, ServiceStatus, Strategy, composite, targets::TargetList};

/// No new candidate starts after this. One more candidate (engine start up to
/// 7 s, a round up to 7 s) and the two final rounds of the chosen strategy fit
/// in the rest, so selection ends within 3 minutes, the phase 1 target. With 22
/// Flowseal strategies and every check stalled, the list would take about 3:15.
pub const CANDIDATE_DEADLINE: Duration = Duration::from_secs(140);

pub trait Backend: Send {
    fn status(&mut self) -> Result<ServiceStatus>;
    fn strategies(&self) -> Result<Vec<Strategy>>;
    fn disable(&mut self) -> Result<()>;
    fn enable(&mut self) -> impl Future<Output = Result<()>> + Send;
    fn set_strategy(&mut self, id: &str) -> impl Future<Output = Result<()>> + Send;
    /// Store (or remove) the strategy selection assembled from several.
    fn set_composite(&mut self, composite: Option<Strategy>) -> Result<()>;
}

pub trait Probes: Send + Sync {
    fn run(&self) -> impl Future<Output = Vec<ProbeResult>> + Send;
    /// What the checks test; tells selection which profiles each depends on.
    fn targets(&self) -> TargetList {
        TargetList::built_in()
    }
}
/// The checks of the active bundle's `targets.json`.
pub struct NetworkProbes {
    pub targets: TargetList,
}
impl Probes for NetworkProbes {
    async fn run(&self) -> Vec<ProbeResult> {
        zapret_core::probe::run(&self.targets).await
    }
    fn targets(&self) -> TargetList {
        self.targets.clone()
    }
}

async fn wait_ready<B: Backend>(backend: &Arc<Mutex<B>>) -> anyhow::Result<()> {
    for _ in 0..30 {
        if backend.lock().await.status()?.running {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    anyhow::bail!("Движок не запустился")
}

pub async fn restore_service<B: Backend>(
    backend: &Arc<Mutex<B>>,
    original: &ServiceStatus,
) -> anyhow::Result<()> {
    backend.lock().await.disable()?;
    if !original.strategy.is_empty() {
        backend
            .lock()
            .await
            .set_strategy(&original.strategy)
            .await?;
    }
    if original.running {
        backend.lock().await.enable().await?;
    }
    Ok(())
}

/// How one candidate went: which checks passed in the first round.
struct Round {
    id: String,
    passed: BTreeSet<String>,
}

/// Start `id` and wait until the engine runs; the reason on failure.
async fn try_start<B: Backend>(backend: &Arc<Mutex<B>>, id: &str) -> Result<()> {
    backend.lock().await.disable()?;
    backend.lock().await.set_strategy(id).await?;
    let enabled = backend.lock().await.enable().await;
    match enabled {
        Ok(()) => wait_ready(backend).await,
        Err(e) => Err(e),
    }
}

fn first_line(e: &anyhow::Error) -> String {
    let reason = e.to_string();
    reason.lines().next().unwrap_or_default().trim().to_owned()
}

fn passed(results: &[ProbeResult]) -> BTreeSet<String> {
    results
        .iter()
        .filter(|r| r.ok)
        .map(|r| r.id.clone())
        .collect()
}

/// `focus`: repair only these checks, keeping the running strategy as the
/// base for everything else; `None`: full selection.
pub async fn auto_select<B: Backend, P: Probes>(
    state: &OperationManager,
    backend: &Arc<Mutex<B>>,
    probes: &P,
    focus: Option<BTreeSet<String>>,
) -> anyhow::Result<()> {
    let original = backend.lock().await.status()?;
    if focus.is_some() && !original.running {
        anyhow::bail!("Включите обход, чтобы починить отдельный сервис");
    }
    let targets = probes.targets();
    let catalog = backend.lock().await.strategies()?;
    // The previous composite comes back if this selection is rolled back.
    let previous_composite = catalog.iter().find(|s| s.id == composite::ID).cloned();
    let current = catalog.iter().find(|s| s.id == original.strategy).cloned();
    let candidates: Vec<_> = catalog
        .into_iter()
        .filter(|s| s.supported && s.id != composite::ID)
        .collect();
    if candidates.is_empty() {
        anyhow::bail!("Нет совместимых стратегий");
    }
    let recovery_path = state.recovery_path.clone();
    tokio::fs::write(&recovery_path, serde_json::to_vec(&original)?).await?;
    state.operation.lock().await.total = candidates.len();
    let started_at = tokio::time::Instant::now();
    // Candidates whose engine did not start, with the first line of the reason.
    let mut skipped = Vec::new();
    // Candidates tried before the time limit stopped the search.
    let mut stopped_after: Option<usize> = None;
    // Checks that nothing made pass, for the final notice.
    let mut unhelped: Vec<String> = Vec::new();
    // Always roll back on cancellation, failed probes, or a service/IPC error.
    let outcome = async {
        // Full selection starts from no bypass; a repair from the strategy
        // that runs now, whose working checks must keep working.
        if focus.is_none() {
            backend.lock().await.disable()?;
        }
        state.operation.lock().await.stage = if focus.is_some() {
            "Проверяем, что перестало работать".into()
        } else {
            "Проверяем доступность без обхода".into()
        };
        let baseline = probes.run().await;
        let names: BTreeMap<String, String> = baseline
            .iter()
            .map(|r| (r.id.clone(), r.name.clone()))
            .collect();
        let keep: BTreeSet<String> = if focus.is_some() {
            passed(&baseline)
        } else {
            BTreeSet::new()
        };
        // Checks that fail now: what selection has to fix.
        let needed: BTreeSet<String> = baseline
            .iter()
            .filter(|r| !r.ok && focus.as_ref().is_none_or(|f| f.contains(&r.id)))
            .map(|r| r.id.clone())
            .collect();
        state.operation.lock().await.results = baseline;
        if needed.is_empty() {
            state.operation.lock().await.notice = Some(if focus.is_some() {
                "Сейчас всё работает, текущая стратегия оставлена.".into()
            } else {
                "Все проверенные соединения доступны без обхода. Исходная настройка сохранена."
                    .into()
            });
            return Ok(None);
        }
        let mut rounds: Vec<Round> = Vec::new();
        // First candidate that made each needed check pass.
        let mut winners: BTreeMap<String, String> = BTreeMap::new();
        for (index, s) in candidates.iter().enumerate() {
            if state.cancelled.load(Ordering::Acquire) {
                return Ok(None);
            }
            if started_at.elapsed() >= CANDIDATE_DEADLINE {
                stopped_after = Some(index);
                break;
            }
            // A repair already knows how the running strategy does.
            if focus.is_some() && s.id == original.strategy {
                continue;
            }
            {
                let mut op = state.operation.lock().await;
                op.current = index + 1;
                op.stage = format!("Проверяем {}", s.name);
            }
            // One strategy the engine rejects must not end the whole search.
            if let Err(e) = try_start(backend, &s.id).await {
                skipped.push(format!("{} ({})", s.name, first_line(&e)));
                continue;
            }
            // New clients and sockets per round prevent connection reuse between candidates.
            let first = probes.run().await;
            let first_pass = first.iter().all(|r| r.ok);
            let ok = passed(&first);
            state.operation.lock().await.results = first;
            if first_pass && !state.cancelled.load(Ordering::Acquire) {
                let confirmation = probes.run().await;
                let all = confirmation.iter().all(|r| r.ok);
                state.operation.lock().await.results = confirmation;
                if all {
                    return Ok::<_, anyhow::Error>(Some(s.id.clone()));
                }
            }
            for check in needed.intersection(&ok) {
                winners.entry(check.clone()).or_insert_with(|| s.id.clone());
            }
            rounds.push(Round {
                id: s.id.clone(),
                passed: ok,
            });
            // Every needed check has its strategy and each can be swapped in:
            // the rest of the list cannot do better than the composite.
            let composable = needed
                .iter()
                .all(|c| winners.contains_key(c) && !targets.slots(c).is_empty());
            let distinct: BTreeSet<_> = winners.values().collect();
            if composable && (distinct.len() > 1 || focus.is_some()) {
                break;
            }
        }
        if state.cancelled.load(Ordering::Acquire) || winners.is_empty() {
            return Ok(None);
        }
        // Nobody passed everything: best strategy as the base, each check's
        // profile from the strategy that made it pass.
        // The first strategy, in bundle order, that fixed the most checks.
        let fixed = |r: &Round| needed.intersection(&r.passed).count();
        let top = rounds.iter().map(fixed).max().unwrap_or(0);
        let base_id = rounds
            .iter()
            .find(|r| fixed(r) == top)
            .map(|r| r.id.clone())
            .expect("a winner implies a round");
        let donors: BTreeMap<String, &Strategy> = winners
            .iter()
            .filter_map(|(check, id)| {
                Some((check.clone(), candidates.iter().find(|s| &s.id == id)?))
            })
            .collect();
        let base = match (&focus, &current) {
            (Some(_), Some(current)) => current,
            _ => {
                let best = candidates
                    .iter()
                    .find(|s| s.id == base_id)
                    .expect("candidate");
                // A winner without per-service profiles (one profile for all
                // TCP) cannot lend them to another base, but can be the base
                // itself: take the first base that every winner can complete.
                std::iter::once(best)
                    .chain(rounds.iter().filter_map(|r| {
                        let s = donors.values().find(|s| s.id == r.id)?;
                        Some(*s)
                    }))
                    .find(|b| composite::unfilled(b, &donors, &targets).is_empty())
                    .unwrap_or(best)
            }
        };
        let tested = match composite::compose(base, &donors, &targets) {
            Some(assembled) => {
                backend.lock().await.set_composite(Some(assembled))?;
                composite::ID.to_owned()
            }
            None => base.id.clone(),
        };
        state.operation.lock().await.stage = if tested == composite::ID {
            "Проверяем стратегию, собранную по сервисам".into()
        } else {
            format!("Проверяем {} ещё раз", base.name)
        };
        if let Err(e) = try_start(backend, &tested).await {
            skipped.push(format!("{} ({})", composite::NAME, first_line(&e)));
            return Ok(None);
        }
        // The composite must pass, twice, everything some strategy fixed and,
        // in a repair, everything that worked before.
        let reachable: BTreeSet<String> = winners.keys().cloned().collect();
        let required: BTreeSet<String> = reachable.union(&keep).cloned().collect();
        for _ in 0..2 {
            if state.cancelled.load(Ordering::Acquire) {
                return Ok(None);
            }
            let results = probes.run().await;
            let ok = passed(&results);
            state.operation.lock().await.results = results;
            if !required.is_subset(&ok) {
                return Ok(None);
            }
        }
        unhelped = needed
            .difference(&reachable)
            .map(|c| names.get(c).cloned().unwrap_or_else(|| c.clone()))
            .collect();
        Ok(Some(tested))
    }
    .await;
    let elapsed = started_at.elapsed().as_secs();
    let took = format!("Подбор занял {} мин {} с.", elapsed / 60, elapsed % 60);
    let limit = stopped_after.map(|tried| {
        format!(
            "Подбор ограничен 3 минутами: проверено {tried} из {} стратегий.",
            candidates.len()
        )
    });
    match outcome {
        Ok(Some(winner)) if !state.cancelled.load(Ordering::Acquire) => {
            if winner != composite::ID {
                // A single strategy won; an old composite is no longer used.
                backend.lock().await.set_composite(None)?;
            }
            let mut notes = vec![took];
            notes.extend(limit.clone());
            if !unhelped.is_empty() {
                notes.push(format!(
                    "Не помогла ни одна стратегия: {}. Возможно, блокировка по адресу, обход её не снимает.",
                    unhelped.join(", ")
                ));
            }
            if !skipped.is_empty() {
                notes.push(format!(
                    "Не запустились и пропущены: {}.",
                    skipped.join("; ")
                ));
            }
            let mut op = state.operation.lock().await;
            op.winner = Some(winner);
            op.notice = Some(notes.join(" "));
            drop(op);
            tokio::fs::remove_file(&recovery_path).await?;
            Ok(())
        }
        other => {
            let restored = async {
                backend.lock().await.set_composite(previous_composite)?;
                restore_service(backend, &original).await
            }
            .await;
            restored.map_err(|e| {
                anyhow::anyhow!("Не удалось восстановить исходную настройку: {e:#}")
            })?;
            tokio::fs::remove_file(&recovery_path).await?;
            let took = match &limit {
                Some(limit) => format!("{took} {limit}"),
                None => took,
            };
            match other {
                Err(e) => Err(e),
                _ if state.cancelled.load(Ordering::Acquire) => Ok(()),
                _ if state.operation.lock().await.notice.is_some() => Ok(()),
                _ if skipped.is_empty() => anyhow::bail!(
                    "Ни одна стратегия не прошла все проверки. {took} Исходная настройка восстановлена."
                ),
                _ => anyhow::bail!(
                    "Ни одна стратегия не прошла все проверки. Не запустились: {}. {took} Исходная настройка восстановлена.",
                    skipped.join("; ")
                ),
            }
        }
    }
}
