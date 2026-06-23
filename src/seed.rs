//! Sample-data loading (feature `seed`). Parses `orchestra_sample_data.json`
//! into a sequence of [`Command`]s, so even bootstrapping flows through `apply`
//! and is subject to the same invariants. This is the ONLY module that touches
//! serde; the core stays serialization-free.

use serde::Deserialize;

use crate::apply::apply;
use crate::command::Command;
use crate::entity::{Chair, EventKind, Program, Tier, Work};
use crate::error::{KernelError, Warning};
use crate::ids::{ConcertId, MusicianId, OrchestraId, VenueId};
use crate::state::Federation;
use crate::time::{Date, Time, TimeSlot};

/// Errors that can occur while turning sample JSON into commands.
#[derive(Debug)]
pub enum SeedError {
    Parse(serde_json::Error),
    Apply(KernelError),
    BadField(String),
}

impl std::fmt::Display for SeedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SeedError::Parse(e) => write!(f, "json parse error: {e}"),
            SeedError::Apply(e) => write!(f, "apply error during seed: {e}"),
            SeedError::BadField(s) => write!(f, "bad field: {s}"),
        }
    }
}

impl std::error::Error for SeedError {}

impl From<serde_json::Error> for SeedError {
    fn from(e: serde_json::Error) -> Self {
        SeedError::Parse(e)
    }
}

// ---- JSON DTOs (private; serde lives only here) -------------------------------

#[derive(Deserialize)]
struct Root {
    organization: Organization,
    venues: Vec<VenueDto>,
    roster_pool: Vec<RosterDto>,
    season_concerts: Vec<ConcertDto>,
}

#[derive(Deserialize)]
struct Organization {
    name: String,
}

#[derive(Deserialize)]
struct VenueDto {
    venue_id: String,
    name: String,
    #[serde(default)]
    capacity: u32,
    #[serde(default)]
    stage_type: String,
    #[serde(default)]
    has_pit: bool,
    #[serde(default)]
    has_organ: bool,
    #[serde(default)]
    loading_dock: bool,
}

#[derive(Deserialize)]
struct RosterDto {
    id: String,
    name: String,
    instrument: String,
    chair: String,
    #[allow(dead_code)]
    tier: String,
    #[serde(default = "one")]
    availability: f64,
}

fn one() -> f64 {
    1.0
}

#[derive(Deserialize)]
struct ConcertDto {
    concert_id: String,
    series: String,
    title: String,
    #[serde(default)]
    program: Vec<WorkDto>,
    #[serde(default)]
    players_required: u16,
    #[serde(default)]
    player_ids: Vec<String>,
    #[serde(default)]
    rehearsals: Vec<EventDto>,
    #[serde(default)]
    performances: Vec<EventDto>,
}

#[derive(Deserialize)]
struct WorkDto {
    composer: String,
    work: String,
    #[serde(default)]
    duration_min: u16,
    #[serde(default)]
    forces: String,
}

#[derive(Deserialize)]
struct EventDto {
    date: String,
    #[serde(default)]
    start_time: String,
    #[serde(default)]
    call_time: String,
    #[serde(default)]
    downbeat: String,
    #[serde(default)]
    duration_hours: f64,
    venue_id: String,
}

// ---- Mapping helpers ----------------------------------------------------------

const ORCHESTRA_ID: &str = "RSO";

fn parse_date(s: &str) -> Result<Date, SeedError> {
    // "2024-09-14"
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 3 {
        return Err(SeedError::BadField(format!("date: {s}")));
    }
    let year = parts[0]
        .parse()
        .map_err(|_| SeedError::BadField(format!("year: {s}")))?;
    let month = parts[1]
        .parse()
        .map_err(|_| SeedError::BadField(format!("month: {s}")))?;
    let day = parts[2]
        .parse()
        .map_err(|_| SeedError::BadField(format!("day: {s}")))?;
    Ok(Date::new(year, month, day))
}

fn parse_time(s: &str) -> Result<Time, SeedError> {
    // "19:30"
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        return Err(SeedError::BadField(format!("time: {s}")));
    }
    let h: u8 = parts[0]
        .parse()
        .map_err(|_| SeedError::BadField(format!("hour: {s}")))?;
    let m: u8 = parts[1]
        .parse()
        .map_err(|_| SeedError::BadField(format!("minute: {s}")))?;
    Time::from_hm(h, m).map_err(|e| SeedError::BadField(format!("{e}: {s}")))
}

fn chair_of(s: &str) -> Chair {
    if s.contains("Concertmaster") {
        Chair::Concertmaster
    } else if s.contains("Principal") {
        Chair::Principal
    } else {
        Chair::Section
    }
}

fn tier_of(s: &str) -> Tier {
    match s {
        "core" => Tier::Core,
        "sub" => Tier::Sub,
        _ => Tier::Extra,
    }
}

/// Parses the sample JSON into a replayable command stream.
///
/// # Errors
/// Returns [`SeedError`] if the JSON is malformed or a date/time field is invalid.
// A flat, top-to-bottom translation of the sample JSON into the command stream; the
// length tracks the sample schema's surface, not branching complexity.
#[allow(clippy::too_many_lines)]
pub fn sample_commands(json: &str) -> Result<Vec<Command>, SeedError> {
    let root: Root = serde_json::from_str(json)?;
    let mut cmds = Vec::new();

    cmds.push(Command::FoundOrchestra {
        id: OrchestraId::new(ORCHESTRA_ID),
        name: root.organization.name,
    });

    for v in root.venues {
        cmds.push(Command::RegisterVenue {
            id: VenueId::new(v.venue_id),
            name: v.name,
            capacity: v.capacity,
            stage_type: v.stage_type,
            has_pit: v.has_pit,
            has_organ: v.has_organ,
            loading_dock: v.loading_dock,
        });
    }

    for r in root.roster_pool {
        // Clamped to [0, 100] before the cast, so the narrowing conversion is exact.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let availability_pct = (r.availability * 100.0).round().clamp(0.0, 100.0) as u8;
        cmds.push(Command::RegisterMusician {
            id: MusicianId::new(r.id.clone()),
            name: r.name,
            primary_instrument: r.instrument.clone(),
            availability_pct,
        });
        cmds.push(Command::AddToRoster {
            orchestra: OrchestraId::new(ORCHESTRA_ID),
            musician: MusicianId::new(r.id),
            instrument: r.instrument,
            chair: chair_of(&r.chair),
            tier: tier_of(&r.tier),
        });
    }

    for c in root.season_concerts {
        let requires_organ = c.program.iter().any(|w| {
            w.forces.to_lowercase().contains("organ") || w.work.to_lowercase().contains("organ")
        });
        let works = c
            .program
            .iter()
            .map(|w| Work {
                composer: w.composer.clone(),
                title: w.work.clone(),
                duration_min: w.duration_min,
                forces: w.forces.clone(),
            })
            .collect();
        cmds.push(Command::ProgramConcert {
            id: ConcertId::new(c.concert_id.clone()),
            orchestra: OrchestraId::new(ORCHESTRA_ID),
            series: c.series,
            title: c.title,
            program: Program {
                works,
                requires_organ,
                requires_pit: false,
            },
            players_required: c.players_required,
        });

        for reh in &c.rehearsals {
            let date = parse_date(&reh.date)?;
            let start = parse_time(if reh.start_time.is_empty() {
                "10:00"
            } else {
                &reh.start_time
            })?;
            // Sample durations are small positives; Rust's float-to-int cast saturates,
            // so this narrowing is safe.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let duration_min = (reh.duration_hours * 60.0).round() as u16;
            cmds.push(Command::ScheduleEvent {
                concert: ConcertId::new(c.concert_id.clone()),
                kind: EventKind::Rehearsal,
                slot: TimeSlot::new(date, start, duration_min.max(1)),
                venue: VenueId::new(reh.venue_id.clone()),
                call_time: None,
                downbeat: None,
            });
        }
        for perf in &c.performances {
            let date = parse_date(&perf.date)?;
            let call = if perf.call_time.is_empty() {
                None
            } else {
                Some(parse_time(&perf.call_time)?)
            };
            let downbeat = if perf.downbeat.is_empty() {
                None
            } else {
                Some(parse_time(&perf.downbeat)?)
            };
            // Occupied window: from call (or start) for 3h by default.
            let start = if let Some(t) = call {
                t
            } else if perf.start_time.is_empty() {
                Time(1170)
            } else {
                parse_time(&perf.start_time)?
            };
            cmds.push(Command::ScheduleEvent {
                concert: ConcertId::new(c.concert_id.clone()),
                kind: EventKind::Performance,
                slot: TimeSlot::new(date, start, 180),
                venue: VenueId::new(perf.venue_id.clone()),
                call_time: call,
                downbeat,
            });
        }

        for pid in c.player_ids {
            cmds.push(Command::AssignPlayer {
                concert: ConcertId::new(c.concert_id.clone()),
                musician: MusicianId::new(pid),
            });
        }
    }

    Ok(cmds)
}

/// Builds a `Federation` by replaying the sample command stream through `apply`,
/// collecting any soft warnings. Hard errors abort with [`SeedError::Apply`].
///
/// # Errors
/// Returns [`SeedError`] on parse failure or any hard conflict during replay.
pub fn build_sample(json: &str) -> Result<(Federation, Vec<Warning>), SeedError> {
    let cmds = sample_commands(json)?;
    let mut state = Federation::new();
    let mut warnings = Vec::new();
    for cmd in cmds {
        let t = apply(&state, cmd).map_err(SeedError::Apply)?;
        state = t.state;
        warnings.extend(t.warnings);
    }
    Ok((state, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINI: &str = r#"{
        "organization": { "name": "Riverside Symphony" },
        "venues": [{ "venue_id": "VEN-01", "name": "Hall", "capacity": 1800, "has_organ": true }],
        "roster_pool": [
            { "id": "M001", "name": "James", "instrument": "Violin I", "chair": "Principal/Concertmaster", "tier": "core", "availability": 1.0 },
            { "id": "M002", "name": "Amy", "instrument": "Violin I", "chair": "Section", "tier": "sub", "availability": 0.4 }
        ],
        "season_concerts": [{
            "concert_id": "C01", "series": "Masterworks", "title": "Gala",
            "program": [{ "composer": "Barber", "work": "Adagio", "duration_min": 9, "forces": "strings" }],
            "players_required": 2,
            "rehearsals": [{ "date": "2024-08-29", "start_time": "10:00", "duration_hours": 3.0, "venue_id": "VEN-01" }],
            "performances": [{ "date": "2024-09-14", "start_time": "19:30", "call_time": "18:00", "downbeat": "19:00", "venue_id": "VEN-01" }],
            "player_ids": ["M001", "M002"]
        }]
    }"#;

    #[test]
    fn mini_builds_without_hard_errors() {
        let (state, warnings) = build_sample(MINI).unwrap();
        assert_eq!(state.musicians.len(), 2);
        assert_eq!(state.concerts[&ConcertId::new("C01")].assignments.len(), 2);
        // M002 has availability 0.4 → LowAvailability warning expected.
        assert!(warnings.iter().any(|w| matches!(
            w,
            Warning::LowAvailability {
                availability_pct: 40,
                ..
            }
        )));
    }

    #[test]
    fn availability_float_maps_to_percentage() {
        let cmds = sample_commands(MINI).unwrap();
        assert!(cmds.iter().any(|c| matches!(
            c,
            Command::RegisterMusician {
                availability_pct: 40,
                ..
            }
        )));
    }
}
