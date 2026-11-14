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
use crate::Strategy;
use std::collections::BTreeMap;

pub const ID: &str = "composite";
pub const NAME: &str = "Подобрано по сервисам";

/// Profiles a check depends on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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

/// Slots each check (probe id) depends on. Checks not listed (internet)
/// need no bypass.
pub fn slots(check: &str) -> &'static [Slot] {
    match check {
        "youtube" => &[Slot::Google],
        "discord" => &[Slot::GeneralTcp, Slot::DiscordMedia],
        "udp" => &[Slot::DiscordVoice],
        "cloudflare" => &[Slot::IpsetTcp],
        _ => &[],
    }
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

/// The composite of `base` with each check's slots taken from its winner
/// (`check id → strategy`). `None` when nothing differs from `base`.
pub fn compose(base: &Strategy, winners: &BTreeMap<String, &Strategy>) -> Option<Strategy> {
    let mut chosen: BTreeMap<Slot, &Strategy> = BTreeMap::new();
    for (check, strategy) in winners {
        if strategy.id == base.id {
            continue;
        }
        for slot in slots(check) {
            chosen.entry(*slot).or_insert(strategy);
        }
    }
    let (header, base_profiles) = profiles(base);
    let mut used: BTreeMap<Slot, &str> = BTreeMap::new();
    let mut result: Vec<Vec<String>> = Vec::new();
    for profile in base_profiles {
        let replacement = slot(&profile).and_then(|s| {
            let donor = chosen.get(&s)?;
            let (_, donor_profiles) = profiles(donor);
            let found = donor_profiles
                .into_iter()
                .find(|p| self::slot(p) == Some(s))?;
            used.insert(s, &donor.id);
            Some(found)
        });
        result.push(replacement.unwrap_or(profile));
    }
    if used.is_empty() {
        return None;
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
        if strategy.id != base.id && slots(check).iter().any(|s| used.contains_key(s)) {
            parts.push(format!("{check}: {}", strategy.name));
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
        let composite = compose(&a, &winners).unwrap();
        assert_eq!(composite.id, ID);
        let text = composite.arguments.join(" ");
        assert!(text.starts_with("--wf-tcp=80,443 --wf-udp=443 --filter-udp=443"));
        assert!(text.contains("--hostlist=@lists/list-google.txt --dpi-desync=fake --new"));
        assert!(text.ends_with("--hostlist=@lists/list-general.txt --dpi-desync=fakedsplit"));
        assert_eq!(
            composite.arguments.iter().filter(|a| *a == "--new").count(),
            3
        );
        assert_eq!(composite.description, "основа a; youtube: b");
    }

    #[test]
    fn nothing_to_swap_gives_no_composite() {
        let a = strategy("a", "multisplit", "fakedsplit");
        let winners = BTreeMap::from([("youtube".to_string(), &a), ("internet".to_string(), &a)]);
        assert!(compose(&a, &winners).is_none());
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
        let composite = compose(general, &winners).unwrap();
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
}
