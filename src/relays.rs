use crate::db::{DbPersonRelay, DbRelay, Direction};
use crate::error::Error;
use crate::globals::GLOBALS;
use dashmap::{DashMap, DashSet};
use nostr_types::{PublicKeyHex, RelayUrl, Unixtime};
use std::fmt;
use std::sync::atomic::{AtomicU8, Ordering};

/// A RelayAssignment is a record of a relay which is serving (or will serve) the general
/// feed for a set of public keys.
#[derive(Debug, Clone)]
pub struct RelayAssignment {
    pub relay_url: RelayUrl,
    pub pubkeys: Vec<PublicKeyHex>,
}

impl RelayAssignment {
    pub fn merge_in(&mut self, other: RelayAssignment) -> Result<(), Error> {
        if self.relay_url != other.relay_url {
            return Err(Error::General(
                "Attempted to merge relay assignments on different relays".to_owned(),
            ));
        }
        self.pubkeys.extend(other.pubkeys);
        Ok(())
    }
}

/// Ways that pick() can fail
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
pub enum RelayPickFailure {
    /// No relays to pick from
    NoRelays,

    /// No people left to assign. A good result.
    NoPeopleLeft,

    /// No progress was made. A stuck result.
    NoProgress,
}

impl fmt::Display for RelayPickFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            RelayPickFailure::NoRelays => write!(f, "No relays to pick from."),
            RelayPickFailure::NoPeopleLeft => write!(f, "All people accounted for."),
            RelayPickFailure::NoProgress => write!(f, "Unable to make further progress."),
        }
    }
}

/// The RelayTracker is a structure that helps assign people we follow to relays we watch.
/// It remembers which publickeys are assigned to which relays, which pubkeys need more
/// relays and how many, which relays need a time out, and person-relay scores for making
/// good assignments dynamically.
#[derive(Debug, Default)]
pub struct RelayTracker {
    /// All of the relays we might use
    pub all_relays: DashMap<RelayUrl, DbRelay>,

    /// All of the relays actually connected
    pub connected_relays: DashSet<RelayUrl>,

    /// All of the relays currently connected, with optional assignments.
    /// (Sometimes a relay is connected for a different kind of subscription.)
    pub relay_assignments: DashMap<RelayUrl, RelayAssignment>,

    /// Relays which recently failed and which require a timeout before
    /// they can be chosen again.  The value is the time when it can be removed
    /// from this list.
    pub excluded_relays: DashMap<RelayUrl, i64>,

    /// For each followed pubkey that still needs assignments, the number of relay
    /// assignments it is seeking.  These start out at settings.num_relays_per_person
    /// (if the person doesn't have that many relays, it will do the best it can)
    pub pubkey_counts: DashMap<PublicKeyHex, u8>,

    /// A ranking of relays per person.
    pub person_relay_scores: DashMap<PublicKeyHex, Vec<(RelayUrl, u64)>>,

    /// Number of relays per person. Taken from settings and fixed.
    pub num_relays_per_person: AtomicU8,
}

impl RelayTracker {
    /// This starts a new RelayTracker that has:
    ///  * All relays
    ///  * All followed public keys, with count starting at num_relays_per_person
    ///  * person relay scores for all person-relay pairings
    pub async fn init(&self) -> Result<(), Error> {
        // just in case it is re-initialized (not sure why it would be)
        self.all_relays.clear();
        self.connected_relays.clear();
        self.relay_assignments.clear();
        self.excluded_relays.clear();
        self.pubkey_counts.clear();
        self.person_relay_scores.clear();

        // Load relays from the database
        for (relay_url, dbrelay) in DbRelay::fetch(None)
            .await?
            .drain(..)
            .map(|dbr| (dbr.url.clone(), dbr))
        {
            self.all_relays.insert(relay_url, dbrelay);
        }

        self.num_relays_per_person.store(
            GLOBALS.settings.read().await.num_relays_per_person,
            Ordering::Relaxed,
        );

        self.refresh_person_relay_scores(true).await?;

        Ok(())
    }

    pub fn add_someone(&self, pubkey: PublicKeyHex) -> Result<(), Error> {
        // Check if we already have them
        if self.pubkey_counts.get(&pubkey).is_some() {
            return Ok(());
        }
        for elem in self.relay_assignments.iter() {
            let assignment = elem.value();
            if assignment.pubkeys.contains(&pubkey) {
                return Ok(());
            }
        }

        self.pubkey_counts
            .insert(pubkey, self.num_relays_per_person.load(Ordering::Relaxed));
        Ok(())
    }

    pub fn remove_someone(&self, pubkey: PublicKeyHex) {
        // Remove from pubkey counts
        self.pubkey_counts.remove(&pubkey);

        // Remove from relay assignments
        for mut elem in self.relay_assignments.iter_mut() {
            let assignment = elem.value_mut();
            if let Some(pos) = assignment.pubkeys.iter().position(|x| x == &pubkey) {
                assignment.pubkeys.remove(pos);
            }
        }

        // This doesn't indicate that the assignment has changed, so the relay is
        // still delivering their events. But the feed shouldn't be showing them.
    }

    pub async fn refresh_person_relay_scores(&self, initialize_counts: bool) -> Result<(), Error> {
        self.person_relay_scores.clear();

        if initialize_counts {
            self.pubkey_counts.clear();
        }

        // Get all the people we follow
        let pubkeys: Vec<PublicKeyHex> = GLOBALS
            .people
            .get_followed_pubkeys()
            .iter()
            .map(|p| p.to_owned())
            .collect();

        // Compute scores for each person_relay pairing
        for pubkey in &pubkeys {
            let best_relays: Vec<(RelayUrl, u64)> =
                DbPersonRelay::get_best_relays(pubkey.to_owned(), Direction::Write).await?;
            self.person_relay_scores.insert(pubkey.clone(), best_relays);

            if initialize_counts {
                self.pubkey_counts.insert(
                    pubkey.clone(),
                    self.num_relays_per_person.load(Ordering::Relaxed),
                );
            }
        }

        Ok(())
    }

    /// When a relay disconnects, call this so that whatever assignments it might have
    /// had can be reassigned.  Then call pick_relays() again.
    pub fn relay_disconnected(&self, url: &RelayUrl) {
        // Remove from connected relays list
        if let Some((_key, assignment)) = self.relay_assignments.remove(url) {
            // Exclude the relay for the next 30 seconds
            let hence = Unixtime::now().unwrap().0 + 30;
            self.excluded_relays.insert(url.to_owned(), hence);
            tracing::debug!("{} goes into the penalty box until {}", url, hence,);

            // Put the public keys back into pubkey_counts
            for pubkey in assignment.pubkeys.iter() {
                self.pubkey_counts
                    .entry(pubkey.to_owned())
                    .and_modify(|e| *e += 1)
                    .or_insert(1);
            }
        }
    }

    /// Create the next assignment, and return the RelayUrl that has it.
    /// The caller is responsible for making that assignment actually happen.
    pub async fn pick(&self) -> Result<RelayUrl, RelayPickFailure> {
        // If we are at max relays, only consider relays we are already
        // connected to
        let max_relays = GLOBALS.settings.read().await.max_relays as usize;
        let at_max_relays = self.relay_assignments.len() >= max_relays;

        // Maybe include excluded relays
        let now = Unixtime::now().unwrap().0;
        self.excluded_relays.retain(|_, v| *v > now);

        if self.pubkey_counts.is_empty() {
            return Err(RelayPickFailure::NoPeopleLeft);
        }

        if self.all_relays.is_empty() {
            return Err(RelayPickFailure::NoRelays);
        }

        // Keep score for each relay
        let scoreboard: DashMap<RelayUrl, u64> = self
            .all_relays
            .iter()
            .map(|x| (x.key().to_owned(), 0))
            .collect();

        // Assign scores to relays from each pubkey
        for elem in self.person_relay_scores.iter() {
            let pubkeyhex = elem.key();
            let relay_scores = elem.value();

            // Skip if this pubkey doesn't need any more assignments
            if let Some(pkc) = self.pubkey_counts.get(pubkeyhex) {
                if *pkc == 0 {
                    // person doesn't need anymore
                    continue;
                }
            } else {
                continue; // person doesn't need any
            }

            // Add scores of their relays
            for (relay, score) in relay_scores.iter() {
                // Skip relays that are excluded
                if self.excluded_relays.contains_key(relay) {
                    continue;
                }

                // If at max, skip relays not already connected
                if at_max_relays && !self.connected_relays.contains(relay) {
                    continue;
                }

                // Skip if relay is already assigned this pubkey
                if let Some(assignment) = self.relay_assignments.get(relay) {
                    if assignment.pubkeys.contains(pubkeyhex) {
                        continue;
                    }
                }

                // Add the score
                if let Some(mut entry) = scoreboard.get_mut(relay) {
                    *entry += score;
                }
            }
        }

        // Adjust all scores based on relay rank and relay success rate
        for mut score_entry in scoreboard.iter_mut() {
            let url = score_entry.key().to_owned();
            let score = score_entry.value_mut();
            if let Some(relay) = self.all_relays.get(&url) {
                let success_rate = relay.success_rate();
                let rank = (relay.rank as f32 * (1.3 * success_rate)) as u64;
                *score *= rank;
            }
        }

        let winner = scoreboard
            .iter()
            .max_by(|x, y| x.value().cmp(y.value()))
            .unwrap();
        let winning_url: RelayUrl = winner.key().to_owned();
        let winning_score: u64 = *winner.value();

        if winning_score == 0 {
            return Err(RelayPickFailure::NoProgress);
        }

        // Now sort out which public keys go with that relay (we did this already
        // above when assigning scores, but in a way which would require a lot of
        // storage to keep, so we just do it again)
        let covered_public_keys = {
            let pubkeys_seeking_relays: Vec<PublicKeyHex> = self
                .pubkey_counts
                .iter()
                .filter(|e| *e.value() > 0)
                .map(|e| e.key().to_owned())
                .collect();

            let mut covered_pubkeys: Vec<PublicKeyHex> = Vec::new();

            for pubkey in pubkeys_seeking_relays.iter() {
                // Skip if relay is already assigned this pubkey
                if let Some(assignment) = self.relay_assignments.get(&winning_url) {
                    if assignment.pubkeys.contains(pubkey) {
                        continue;
                    }
                }

                if let Some(elem) = self.person_relay_scores.get(pubkey) {
                    let relay_scores = elem.value();

                    if relay_scores.iter().any(|e| e.0 == winning_url) {
                        covered_pubkeys.push(pubkey.to_owned());

                        if let Some(mut count) = self.pubkey_counts.get_mut(pubkey) {
                            if *count > 0 {
                                *count -= 1;
                            }
                        }
                    }
                }
            }

            covered_pubkeys
        };

        if covered_public_keys.is_empty() {
            return Err(RelayPickFailure::NoProgress);
        }

        // Only keep pubkey_counts that are still > 0
        self.pubkey_counts.retain(|_, count| *count > 0);

        let assignment = RelayAssignment {
            relay_url: winning_url.clone(),
            pubkeys: covered_public_keys,
        };

        // Put assignment into relay_assignments
        if let Some(mut maybe_elem) = self.relay_assignments.get_mut(&winning_url) {
            // FIXME this could cause a panic, but it would mean we have bad code.
            maybe_elem.value_mut().merge_in(assignment).unwrap();
        } else {
            self.relay_assignments
                .insert(winning_url.clone(), assignment);
        }

        Ok(winning_url)
    }
}
