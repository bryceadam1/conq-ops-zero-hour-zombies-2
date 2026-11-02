//! The multiplayer lockstep gate over the player's own install: the harness's match
//! (`zhbo_core::sim::harness`, behind zhbo-core's `harness` feature) over the converted weapon
//! table's whole roster, which the built-in catalog's gate (`zhbo-core` `sim/lockstep_tests.rs`)
//! cannot reach.

#[cfg(test)]
mod tests {
    use zhbo_core::classmenu::items::given_parts;
    use zhbo_core::sim::harness::{play, roster_classes, Match};

    /// Every weapon and alt mode, attachment, equipment and offhand the install's class editor
    /// offers (the crossbow's bolt and the shellshocks among what they do), killstreak-free, on three
    /// players' machines of different local seats plus one that never renders: the same sim hash
    /// and engine calls on every frame. The table is the one the launcher ships
    /// (`generals::files`, installed as the game installs it). DLC items are in (a peer who owns the
    /// pack sends them; the received repair never reads this machine's locks), so peers with
    /// different DLC are covered too. `--nocapture` prints the run's size and time.
    #[test]
    #[ignore = "reads this machine's Black Ops II install (ZHBO_BO2_DIR); converts the weapon table and plays the roster (minutes, release)"]
    fn the_install_roster_plays_in_lockstep() {
        let bo2_dir = zhbo_requirements::check_requirements().black_ops2.unwrap().path;
        crate::test_catalog::install_shipped(&bo2_dir);
        let classes = roster_classes();
        let rounds = (classes.len() as u32).div_ceil(3) + 1;
        let picks = |r: u32, p: i32, pick: u32| classes[((r + pick) as usize * 3 + p as usize) % classes.len()].to_wire_words();
        let started = std::time::Instant::now();
        let report = play(&Match { players: 3, rounds, picks: &picks });
        println!("{} classes, {rounds} rounds, {} frames in {:.0?}; renders {:?}", classes.len(), report.frames, started.elapsed(), report.renders);
        let seen = |label: &str| report.seen.contains(label);
        let base = |id: &str| given_parts(id).0;
        let o = zhbo_core::classmenu::offer();
        let cat = zhbo_core::weapons::catalog();
        let mut missing: Vec<String> = Vec::new();
        for item in o.primary.iter().chain(&o.secondary) {
            // A shot, a launch or a deployed shield with it up, or its projectile in flight (a
            // guided launcher's rocket).
            let fired = report.seen.iter().any(|s| {
                ["fired:", "projectile:", "deployed:"].iter().any(|p| s.strip_prefix(p).is_some_and(|w| base(w.rsplit('~').next().unwrap_or(w)) == *item))
            });
            if !fired {
                missing.push(format!("fired {item}"));
            }
            for att in zhbo_core::classmenu::offered_attachments(item) {
                let worn = report.seen.iter().any(|s| s.strip_prefix("worn:").is_some_and(|w| { let (b, a) = given_parts(w); b == *item && a.iter().any(|x| x == att) }));
                if !worn {
                    missing.push(format!("worn {item}+{att}"));
                }
            }
            for (short, alt) in &cat.get(item).unwrap().alt_modes {
                if !report.seen.iter().any(|s| s.strip_prefix("fired:").is_some_and(|w| w.split('~').next() == Some(alt.as_str()))) {
                    missing.push(format!("alt {item}+{short} ({alt})"));
                }
            }
        }
        for item in o.lethal.iter().chain(&o.tactical) {
            // Thrown, set down, or in flight. The Black Hat (`Place = hack`) is used only on enemy
            // equipment it is aimed at, which the script does not stage: it must be carried
            // (`sim::hack`'s own tests cover the hack).
            let used = seen(&format!("thrown:{item}")) || seen(&format!("equipment:{item}")) || seen(&format!("projectile:{item}"));
            let hack = cat.get(item).is_some_and(|d| d.place == zhbo_core::weapons::Place::Hack);
            if !(used || hack && seen(&format!("carried:{item}"))) {
                missing.push(format!("used {item}"));
            }
        }
        // What the offhands and launchers do: the crossbow's bolt becomes its explosive bolt, and
        // the stun grenades shellshock a General.
        for label in ["projectile:explosive_bolt_mp", "shock:concussion_grenade_mp", "shock:flashbang"] {
            if !seen(label) {
                missing.push(label.to_string());
            }
        }
        assert!(report.renders[0] > report.renders[1] && report.renders[3] == 0, "{:?}", report.renders);
        assert!(missing.is_empty(), "the roster did not reach:\n  {}", missing.join("\n  "));
    }
}
