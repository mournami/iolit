//! Composite strategy: when no single strategy passes every check, each
//! service gets the profile of the strategy that worked for it.
//!
//! A Flowseal strategy is a list of winws profiles separated by `--new`;
//! winws uses the first profile whose filters match a packet. All strategies
//! share the same filters (ports, hostlists, ipsets) and differ in the
//! `--dpi-desync*` options, so each profile is identified by its filters as a
//! slot. The composite takes the strategy that passed the most checks and
//! swaps the profiles in each service's slots for the ones of that service's
//! strategy. The `--wf-*` options (which packets reach winws at all) stay
//! from the base and are the same in every Flowseal strategy.
//!
//! A strategy with one profile for all TCP ("general (ALT5)") has no slots of
//! its own. It can still be the base: the services' profiles then go in front
//! of its catch-all profile, which winws would otherwise match first.
use crate::{Strategy, targets::TargetList};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const ID: &str = "composite";
pub const NAME: &str = "Подобрано по сервисам";

/// Profiles a check depends on; `targets.json` names them per service.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Slot {
    /// TCP 443 to `list-google.txt`: YouTube.
    Google,
    /// TCP 80/443 to `list-general.txt`: Discord, other blocked sites.
    GeneralTcp,
    /// Discord media ports to `discord.media`.
    DiscordMedia,
    /// UDP voice ports with the Discord/STUN payload filter.
    DiscordVoice,
    /// TCP to addresses in `ipset-all.txt`: Cloudflare and others by IP.
    IpsetTcp,
}

fn has(profile: &[String], prefix: &str, needle: &str) -> bool {
    profile
        .iter()
        .any(|a| a.strip_prefix(prefix).is_some_and(|v| v.contains(needle)))
}

/// Which slot a profile fills, from its filters.
pub fn slot(profile: &[String]) -> Option<Slot> {
    let tcp = profile.iter().any(|a| a.starts_with("--filter-tcp="));
    if has(profile, "--filter-l7=", "discord") {
        Some(Slot::DiscordVoice)
    } else if has(profile, "--hostlist-domains=", "discord.media") {
        Some(Slot::DiscordMedia)
    } else if tcp && has(profile, "--hostlist=", "list-google.txt") {
        Some(Slot::Google)
    } else if tcp && has(profile, "--hostlist=", "list-general.txt") {
        Some(Slot::GeneralTcp)
    } else if tcp && has(profile, "--ipset=", "ipset-all.txt") {
        Some(Slot::IpsetTcp)
    } else {
        None
    }
}

/// Profiles of a strategy, without `--new` and without the `--wf-*` options.
fn profiles(strategy: &Strategy) -> (Vec<String>, Vec<Vec<String>>) {
    let mut header = Vec::new();
    let mut profiles = vec![Vec::new()];
    for arg in &strategy.arguments {
        if arg == "--new" {
            profiles.push(Vec::new());
        } else if arg.starts_with("--wf-") {
            header.push(arg.clone());
        } else {
            profiles.last_mut().expect("one profile").push(arg.clone());
        }
    }
    (header, profiles)
}

/// A TCP profile with no host, address or payload filter: it takes every TCP
/// connection on its ports, as in Flowseal's "general (ALT5)". Such a strategy
/// has no per-service slots.
fn catch_all(profile: &[String]) -> bool {
    profile.iter().any(|a| a.starts_with("--filter-tcp="))
        && !profile.iter().any(|a| {
            a.starts_with("--hostlist=")
                || a.starts_with("--hostlist-domains=")
                || a.starts_with("--ipset=")
                || a.starts_with("--filter-l7=")
        })
}

/// The profile `strategy` uses for `slot`, if it has one of its own.
fn own_profile(strategy: &Strategy, slot: Slot) -> Option<Vec<String>> {
    profiles(strategy)
        .1
        .into_iter()
        .find(|p| self::slot(p) == Some(slot))
}

/// The slots each donor gives `base`. A donor gives a slot only from a
/// profile of its own for it. `base` takes it in place of its profile for
/// that slot or, when it has none, ahead of its catch-all profile.
fn plan<'a>(
    base: &Strategy,
    winners: &BTreeMap<String, &'a Strategy>,
    targets: &TargetList,
) -> BTreeMap<Slot, (&'a Strategy, Vec<String>)> {
    let base_profiles = profiles(base).1;
    let has_catch_all = base_profiles.iter().any(|p| catch_all(p));
    let mut chosen = BTreeMap::new();
    for (check, strategy) in winners {
        if strategy.id == base.id {
            continue;
        }
        for &slot in targets.slots(check) {
            if chosen.contains_key(&slot) {
                continue;
            }
            let in_base = base_profiles.iter().any(|p| self::slot(p) == Some(slot));
            if !in_base && !has_catch_all {
                continue;
            }
            if let Some(profile) = own_profile(strategy, slot) {
                chosen.insert(slot, (*strategy, profile));
            }
        }
    }
    chosen
}

/// Checks whose winner can give `base` none of their profiles, so the
/// composite on this base would still fail them.
pub fn unfilled(
    base: &Strategy,
    winners: &BTreeMap<String, &Strategy>,
    targets: &TargetList,
) -> Vec<String> {
    let chosen = plan(base, winners, targets);
    winners
        .iter()
        .filter(|(check, strategy)| {
            strategy.id != base.id
                && targets.slots(check).iter().all(|slot| {
                    chosen
                        .get(slot)
                        .is_none_or(|(donor, _)| donor.id != strategy.id)
                })
        })
        .map(|(check, _)| check.clone())
        .collect()
}

/// The composite of `base` with each check's slots taken from its winner
/// (`check id → strategy`). `None` when nothing differs from `base`.
pub fn compose(
    base: &Strategy,
    winners: &BTreeMap<String, &Strategy>,
    targets: &TargetList,
) -> Option<Strategy> {
    let chosen = plan(base, winners, targets);
    if chosen.is_empty() {
        return None;
    }
    let (header, base_profiles) = profiles(base);
    let present: Vec<Slot> = base_profiles.iter().filter_map(|p| slot(p)).collect();
    let first_catch_all = base_profiles.iter().position(|p| catch_all(p));
    let mut used: BTreeMap<Slot, &str> = BTreeMap::new();
    let mut result: Vec<Vec<String>> = Vec::new();
    for (i, profile) in base_profiles.into_iter().enumerate() {
        if Some(i) == first_catch_all {
            // winws takes the first matching profile, so a service's own
            // profile has to come before the one that takes all TCP.
            for (slot, (donor, own)) in &chosen {
                if !present.contains(slot) {
                    result.push(own.clone());
                    used.insert(*slot, &donor.id);
                }
            }
        }
        let replacement = slot(&profile).and_then(|s| {
            let (donor, own) = chosen.get(&s)?;
            used.insert(s, &donor.id);
            Some(own.clone())
        });
        result.push(replacement.unwrap_or(profile));
    }
    let mut arguments = header;
    for (i, profile) in result.into_iter().enumerate() {
        if i > 0 {
            arguments.push("--new".into());
        }
        arguments.extend(profile);
    }
    let mut parts = vec![format!("основа {}", base.name)];
    for (check, strategy) in winners {
        if strategy.id != base.id && targets.slots(check).iter().any(|s| used.contains_key(s)) {
            let name = targets
                .get(check)
                .map_or(check.as_str(), |t| t.name.as_str());
            parts.push(format!("{name}: {}", strategy.name));
        }
    }
    Some(Strategy {
        id: ID.into(),
        name: NAME.into(),
        description: parts.join("; "),
        supported: true,
        arguments,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strategy(id: &str, google: &str, general: &str) -> Strategy {
        let args = format!(
            "--wf-tcp=80,443 --wf-udp=443 --filter-udp=443 --hostlist=@lists/list-general.txt \
             --dpi-desync=fake --new --filter-udp=19294-19344 --filter-l7=discord,stun \
             --dpi-desync=fake --new --filter-tcp=443 --hostlist=@lists/list-google.txt \
             --dpi-desync={google} --new --filter-tcp=80,443 \
             --hostlist=@lists/list-general.txt --dpi-desync={general}"
        );
        Strategy {
            id: id.into(),
            name: id.into(),
            description: String::new(),
            supported: true,
            arguments: args.split(' ').map(String::from).collect(),
        }
    }

    #[test]
    fn each_service_gets_its_winners_profile() {
        let a = strategy("a", "multisplit", "fakedsplit");
        let b = strategy("b", "fake", "split2");
        let winners = BTreeMap::from([("youtube".to_string(), &b), ("discord".to_string(), &a)]);
        let composite = compose(&a, &winners, &TargetList::built_in()).unwrap();
        assert_eq!(composite.id, ID);
        let text = composite.arguments.join(" ");
        assert!(text.starts_with("--wf-tcp=80,443 --wf-udp=443 --filter-udp=443"));
        assert!(text.contains("--hostlist=@lists/list-google.txt --dpi-desync=fake --new"));
        assert!(text.ends_with("--hostlist=@lists/list-general.txt --dpi-desync=fakedsplit"));
        assert_eq!(
            composite.arguments.iter().filter(|a| *a == "--new").count(),
            3
        );
        assert_eq!(composite.description, "основа a; YouTube: b");
    }

    #[test]
    fn nothing_to_swap_gives_no_composite() {
        let a = strategy("a", "multisplit", "fakedsplit");
        let winners = BTreeMap::from([("youtube".to_string(), &a), ("internet".to_string(), &a)]);
        assert!(compose(&a, &winners, &TargetList::built_in()).is_none());
    }

    #[test]
    fn flowseal_profiles_have_the_expected_slots() {
        let catalog: Vec<Strategy> = serde_json::from_str(include_str!(
            "../../../contracts/golden/flowseal-1.10.4/strategies.json"
        ))
        .unwrap();
        for strategy in &catalog {
            let (header, profiles) = profiles(strategy);
            assert_eq!(header.len(), 2, "{}", strategy.id);
            let found: Vec<_> = profiles.iter().filter_map(|p| slot(p)).collect();
            // Every slot a check needs exists once in nearly every strategy.
            for wanted in [Slot::Google, Slot::GeneralTcp, Slot::DiscordVoice] {
                assert!(
                    found.iter().filter(|s| **s == wanted).count() <= 1,
                    "{} {wanted:?}",
                    strategy.id
                );
            }
        }
        let general = catalog.iter().find(|s| s.id == "general").unwrap();
        let alt = catalog.iter().find(|s| s.id == "general-alt").unwrap();
        let winners = BTreeMap::from([("youtube".to_string(), alt)]);
        let composite = compose(general, &winners, &TargetList::built_in()).unwrap();
        // Same number of profiles; only the Google profile changed.
        let (_, before) = profiles(general);
        let (_, after) = profiles(&composite);
        assert_eq!(before.len(), after.len());
        let changed: Vec<_> = before
            .iter()
            .zip(&after)
            .filter(|(b, a)| b != a)
            .map(|(b, _)| slot(b))
            .collect();
        assert!(
            changed.iter().all(|s| *s == Some(Slot::Google)),
            "{changed:?}"
        );
    }

    fn flowseal() -> Vec<Strategy> {
        serde_json::from_str(include_str!(
            "../../../contracts/golden/flowseal-1.10.4/strategies.json"
        ))
        .unwrap()
    }

    #[test]
    fn catch_all_base_gets_service_profiles_in_front() {
        let catalog = flowseal();
        let alt5 = catalog.iter().find(|s| s.id == "general-alt5").unwrap();
        let general = catalog.iter().find(|s| s.id == "general").unwrap();
        let (_, alt5_profiles) = profiles(alt5);
        assert!(alt5_profiles.iter().any(|p| catch_all(p)));
        assert!(alt5_profiles.iter().all(|p| slot(p) != Some(Slot::Google)));
        // ALT5 fixed Discord and Cloudflare, `general` fixed YouTube.
        let winners = BTreeMap::from([
            ("youtube".to_string(), general),
            ("discord".to_string(), alt5),
        ]);
        let targets = TargetList::built_in();
        assert!(unfilled(alt5, &winners, &targets).is_empty());
        let composite = compose(alt5, &winners, &targets).unwrap();
        let (_, after) = profiles(&composite);
        let google = after
            .iter()
            .position(|p| slot(p) == Some(Slot::Google))
            .expect("google profile added");
        let all_tcp = after.iter().position(|p| catch_all(p)).unwrap();
        assert!(google < all_tcp, "{after:?}");
        assert_eq!(after[google], own_profile(general, Slot::Google).unwrap());
        assert_eq!(after.len(), alt5_profiles.len() + 1);
        assert_eq!(
            composite.description,
            "основа general (ALT5); YouTube: general"
        );
    }

    #[test]
    fn catch_all_winner_cannot_lend_a_slot() {
        let catalog = flowseal();
        let alt5 = catalog.iter().find(|s| s.id == "general-alt5").unwrap();
        let general = catalog.iter().find(|s| s.id == "general").unwrap();
        let winners = BTreeMap::from([("youtube".to_string(), alt5)]);
        let targets = TargetList::built_in();
        // On `general` the YouTube fix is lost; on ALT5 nothing needs a swap.
        assert_eq!(unfilled(general, &winners, &targets), ["youtube"]);
        assert!(compose(general, &winners, &targets).is_none());
        assert!(unfilled(alt5, &winners, &targets).is_empty());
    }
}
