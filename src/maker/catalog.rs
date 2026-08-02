use super::entity_data::EntityKind;
use super::level::{LevelData, LevelTag};
use super::storage::{self, LevelStorage};
use super::thumbnail::{self, ThumbPreview};

/// Grid dimensions for the browse-card preview. cols*rows colored boxes per card.
pub const PREVIEW_COLS: usize = 18;
pub const PREVIEW_ROWS: usize = 13;

/// UI-ready snapshot of one level in the browsable pool (named slots +
/// collection). Kept cheap: no map/entity runtime data, just metadata.
#[derive(Clone, Debug)]
pub struct LevelSummary {
    pub key: String,
    pub name: String,
    pub author: String,
    pub description: String,
    pub tags: Vec<LevelTag>,
    pub block_count: u32,
    pub entity_count: u32,
    pub track_count: u32,
    pub verified: bool,
    pub created_at: u64,
    /// World record in ms (fastest non-author clear); distinct from the maker's
    /// internal verification time, which is never surfaced in the UI.
    pub record_ms: Option<u32>,
    /// Soft "difficulty" 0..=3 from content heuristics (not player ratings).
    pub difficulty: u8,
    /// Bitset of which entity kinds appear in the level (`KIND_*` flags).
    pub kind_flags: u32,
    pub source: LevelSourceKind,
    /// Isometric preview computed from the level data (fresh on every build).
    pub preview: ThumbPreview,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LevelSourceKind {
    Slot,
    Collection,
}

pub const KIND_GLIMMER: u32 = 1 << 0;
pub const KIND_LAUNCH_PAD: u32 = 1 << 1;
pub const KIND_SEAL: u32 = 1 << 2;
pub const KIND_DRIFT_PLATE: u32 = 1 << 3;
pub const KIND_PROWLER: u32 = 1 << 4;
pub const KIND_TRIGGER_ORB: u32 = 1 << 5;
pub const KIND_RELAY_GATE: u32 = 1 << 6;

pub fn kind_flag(kind: EntityKind) -> u32 {
    match kind {
        EntityKind::Glimmer => KIND_GLIMMER,
        EntityKind::LaunchPad => KIND_LAUNCH_PAD,
        EntityKind::Seal => KIND_SEAL,
        EntityKind::DriftPlate => KIND_DRIFT_PLATE,
        EntityKind::Prowler => KIND_PROWLER,
        EntityKind::TriggerOrb => KIND_TRIGGER_ORB,
        EntityKind::RelayGate => KIND_RELAY_GATE,
    }
}

fn summarize(key: String, data: &LevelData, source: LevelSourceKind) -> LevelSummary {
    let mut kind_flags = 0u32;
    for e in &data.entities {
        kind_flags |= kind_flag(e.kind);
    }
    LevelSummary {
        key,
        name: data.name.clone(),
        author: data.author.clone(),
        description: data.description.clone(),
        tags: data.tags.clone(),
        block_count: data.blocks.len() as u32,
        entity_count: data.entities.len() as u32,
        track_count: data.tracks.len() as u32,
        verified: data.is_verified,
        created_at: data.created_at,
        record_ms: data.record_ms,
        difficulty: estimate_difficulty(data),
        kind_flags,
        source,
        preview: thumbnail::render_preview(data, PREVIEW_COLS, PREVIEW_ROWS),
    }
}

/// Soft "difficulty" 0..=3 from content heuristics (not player ratings).
pub fn estimate_difficulty(data: &LevelData) -> u8 {
    let mut score = 0u32;
    score += data.blocks.len() as u32 / 40;
    score += data
        .entities
        .iter()
        .filter(|e| matches!(e.kind, EntityKind::Prowler))
        .count() as u32
        * 2;
    score += data
        .entities
        .iter()
        .filter(|e| matches!(e.kind, EntityKind::Seal | EntityKind::RelayGate))
        .count() as u32;
    score += data.tracks.len() as u32;
    if let Some(t) = data.author_time {
        if t > 90_000 {
            score += 2;
        } else if t > 45_000 {
            score += 1;
        }
    }
    score += data.author_deaths.min(5);
    match score {
        0..=2 => 0, // Easy
        3..=5 => 1, // Normal
        6..=9 => 2, // Hard
        _ => 3,     // Expert
    }
}

pub fn difficulty_label(d: u8) -> &'static str {
    ["Easy", "Normal", "Hard", "Expert"][d.min(3) as usize]
}

/// Enumerates every level the player can browse (named slots + collection),
/// loads each, and returns summaries sorted newest-first. Corrupt or missing
/// files are skipped.
pub fn build_catalog(storage: &LevelStorage) -> Vec<LevelSummary> {
    let mut keys: Vec<String> = storage::list_slots(storage);
    keys.extend(storage::list_collection(storage));

    let mut out = vec![];
    for key in keys {
        let Ok(Some(text)) = storage.0.load(&key) else {
            continue;
        };
        if let Ok(data) = storage::deserialize_level(&text) {
            let source = if key.starts_with(storage::COLLECTION_PREFIX) {
                LevelSourceKind::Collection
            } else {
                LevelSourceKind::Slot
            };
            out.push(summarize(key, &data, source));
        }
    }
    out.sort_by_key(|s| std::cmp::Reverse(s.created_at));
    out
}

#[derive(Clone, Debug, Default)]
pub struct BrowseQuery {
    pub text_terms: Vec<String>, // AND across free terms
    pub name: Option<String>,
    pub author: Option<String>,
    pub id_prefix: Option<String>,
    pub tags_any: Vec<LevelTag>, // from `tag:` / `#` prefixes
    pub verified_only: bool,
    pub difficulty: Option<u8>, // exact
    pub difficulty_min: Option<u8>,
    pub has_kinds: Vec<EntityKind>, // has:prowler etc.
    pub has_tracks: bool,
}

fn parse_tag(s: &str) -> Option<LevelTag> {
    match s {
        "short" => Some(LevelTag::Short),
        "puzzle" => Some(LevelTag::Puzzle),
        "precision" => Some(LevelTag::Precision),
        "chill" => Some(LevelTag::Chill),
        "music" => Some(LevelTag::Music),
        "auto" => Some(LevelTag::Auto),
        _ => None,
    }
}

fn parse_diff_token(rest: &str, q: &mut BrowseQuery) {
    match rest {
        "easy" | "0" => q.difficulty = Some(0),
        "normal" | "1" => q.difficulty = Some(1),
        "hard" | "2" => q.difficulty = Some(2),
        "expert" | "3" => q.difficulty = Some(3),
        s if s.starts_with(">=") => {
            q.difficulty_min = s[2..].parse().ok();
        }
        s => q.difficulty = s.parse().ok(),
    }
}

pub fn parse_browse_query(raw: &str) -> BrowseQuery {
    let mut q = BrowseQuery::default();
    for tok in raw.split_whitespace() {
        let lower = tok.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("name:") {
            q.name = Some(rest.to_string());
        } else if let Some(rest) = lower.strip_prefix("author:") {
            q.author = Some(rest.to_string());
        } else if let Some(rest) = lower.strip_prefix("id:") {
            q.id_prefix = Some(rest.to_string());
        } else if let Some(rest) = lower.strip_prefix("tag:") {
            if let Some(t) = parse_tag(rest) {
                q.tags_any.push(t);
            }
        } else if let Some(rest) = lower.strip_prefix('#') {
            if let Some(t) = parse_tag(rest) {
                q.tags_any.push(t);
            }
        } else if lower == "verified:1" || lower == "verified:true" {
            q.verified_only = true;
        } else if let Some(rest) = lower.strip_prefix("diff:") {
            parse_diff_token(rest, &mut q);
        } else if let Some(rest) = lower.strip_prefix("has:") {
            match rest {
                "track" | "tracks" => q.has_tracks = true,
                "prowler" => q.has_kinds.push(EntityKind::Prowler),
                "gate" | "relay" => q.has_kinds.push(EntityKind::RelayGate),
                "trigger" | "orb" => q.has_kinds.push(EntityKind::TriggerOrb),
                "seal" => q.has_kinds.push(EntityKind::Seal),
                "pad" | "launch" => q.has_kinds.push(EntityKind::LaunchPad),
                "drift" => q.has_kinds.push(EntityKind::DriftPlate),
                "glimmer" => q.has_kinds.push(EntityKind::Glimmer),
                _ => q.text_terms.push(lower),
            }
        } else {
            q.text_terms.push(lower);
        }
    }
    q
}

fn matches_query(s: &LevelSummary, q: &BrowseQuery) -> bool {
    if q.verified_only && !s.verified {
        return false;
    }
    if let Some(ref n) = q.name {
        if !s.name.to_lowercase().contains(n) {
            return false;
        }
    }
    if let Some(ref a) = q.author {
        if !s.author.to_lowercase().contains(a) {
            return false;
        }
    }
    if let Some(ref id) = q.id_prefix {
        if !s.key.to_lowercase().contains(id) {
            return false;
        }
    }
    if !q.tags_any.is_empty() && !q.tags_any.iter().any(|t| s.tags.contains(t)) {
        return false;
    }
    if let Some(d) = q.difficulty {
        if s.difficulty != d {
            return false;
        }
    }
    if let Some(dmin) = q.difficulty_min {
        if s.difficulty < dmin {
            return false;
        }
    }
    if q.has_tracks && s.track_count == 0 {
        return false;
    }
    if !q.has_kinds.is_empty()
        && !q
            .has_kinds
            .iter()
            .all(|k| s.kind_flags & kind_flag(*k) != 0)
    {
        return false;
    }
    for term in &q.text_terms {
        let hit = s.name.to_lowercase().contains(term)
            || s.author.to_lowercase().contains(term)
            || s.description.to_lowercase().contains(term)
            || s.key.to_lowercase().contains(term)
            || s.tags
                .iter()
                .any(|t| t.label().to_lowercase().contains(term.as_str()));
        if !hit {
            return false;
        }
    }
    true
}

/// Filter + sort the full catalog for the browse UI.
///
/// `include_tags` are INCLUDE chips (OR among them, AND with text filters).
/// `verified_only` and `difficulty` come from the filter chips; `raw_query`
/// holds free text plus `name:` / `author:` / `tag:` / `has:` prefixes.
pub fn filter_catalog(
    all: &[LevelSummary],
    raw_query: &str,
    include_tags: &[LevelTag],
    verified_only: bool,
    difficulty: Option<u8>,
    sort: u8,
) -> Vec<LevelSummary> {
    let q = parse_browse_query(raw_query);
    let mut out: Vec<LevelSummary> = all
        .iter()
        .filter(|s| {
            if verified_only && !s.verified {
                return false;
            }
            if let Some(d) = difficulty {
                if s.difficulty != d {
                    return false;
                }
            }
            if !include_tags.is_empty() && !include_tags.iter().any(|t| s.tags.contains(t)) {
                return false;
            }
            matches_query(s, &q)
        })
        .cloned()
        .collect();

    match sort % 6 {
        0 => out.sort_by_key(|l| std::cmp::Reverse(l.created_at)),
        1 => out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
        2 => out.sort_by_key(|l| l.block_count),
        3 => out.sort_by_key(|l| std::cmp::Reverse(l.block_count)),
        4 => out.sort_by(|a, b| match (a.record_ms, b.record_ms) {
            (Some(x), Some(y)) => x.cmp(&y),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            _ => b.created_at.cmp(&a.created_at),
        }),
        _ => out.sort_by_key(|l| std::cmp::Reverse(l.difficulty)),
    }
    out
}
