//! The engine provider registry: identity, names, verified depths and
//! capabilities of every supported engine.
//!
//! Stored data, settings and HTTP contracts keep the engine ID strings
//! (`gnubg`, `bgsage`). Code parses them into [`EngineId`] and asks a
//! capability instead of comparing strings, so each rule that differs between
//! engines is decided once here and a new engine is a compile error at every
//! decision it has to make. Unknown IDs parse to `None` and fail closed; no
//! caller falls back to another engine.

use anyhow::{Result, anyhow};

/// One supported engine. The internal NeonGammon ID stays `bgsage` in stored
/// snapshots, settings and APIs; only its display names say NeonGammon.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum EngineId {
    Gnubg,
    NeonGammon,
}

impl EngineId {
    /// Stored engine ID.
    pub(crate) const fn id(self) -> &'static str {
        match self {
            Self::Gnubg => "gnubg",
            Self::NeonGammon => "bgsage",
        }
    }

    pub(crate) fn parse(id: &str) -> Option<Self> {
        match id {
            "gnubg" => Some(Self::Gnubg),
            "bgsage" => Some(Self::NeonGammon),
            _ => None,
        }
    }

    /// The engine for a stored ID, or an error naming the unsupported ID.
    pub(crate) fn require(id: &str) -> Result<Self> {
        Self::parse(id).ok_or_else(|| anyhow!("Unsupported engine {id}"))
    }

    /// User-facing short name.
    pub(crate) const fn display_name(self) -> &'static str {
        match self {
            Self::Gnubg => "GNUbg",
            Self::NeonGammon => "NeonGammon",
        }
    }

    /// Full product name, as engines name themselves in analysis provenance.
    pub(crate) const fn product_name(self) -> &'static str {
        match self {
            Self::Gnubg => "GNU Backgammon",
            Self::NeonGammon => "NeonGammon",
        }
    }

    /// Verified depths for `purpose` (`analysis`, `play`, `duel`), shared with
    /// client normalization through the configuration options.
    pub(crate) fn depths(self, purpose: &str) -> Vec<i32> {
        crate::configuration::engine_depths(self.id(), purpose)
    }

    pub(crate) fn min_threads(self) -> i32 {
        crate::configuration::engine_min_threads(self.id())
    }

    pub(crate) fn default_threads(self, purpose: &str) -> i32 {
        crate::configuration::engine_default_threads(self.id(), purpose)
    }

    /// GNUbg is the independent rules engine: it validates legal moves and
    /// drives the game loop, and it plays its own turns inside that loop.
    /// Other engines yield to it for every bot decision.
    pub(crate) const fn is_rules_engine(self) -> bool {
        match self {
            Self::Gnubg => true,
            Self::NeonGammon => false,
        }
    }

    /// Whether the GNUbg `set threads` settings apply to this engine.
    /// NeonGammon threads come from its own verified profile.
    pub(crate) const fn uses_gnubg_threads(self) -> bool {
        self.is_rules_engine()
    }

    /// Whether selecting this engine requires a verified sidecar installation
    /// profile for the requested depth and thread count.
    pub(crate) const fn requires_verified_profile(self) -> bool {
        match self {
            Self::Gnubg => false,
            Self::NeonGammon => true,
        }
    }

    /// Whether the engine offers named opponent presets at all. The installed
    /// package decides availability (`SageSnapshot::supports_difficulty_presets`).
    pub(crate) const fn has_difficulty_presets(self) -> bool {
        match self {
            Self::Gnubg => false,
            Self::NeonGammon => true,
        }
    }

    /// Whether the selective top-2 bot search is available at this depth.
    pub(crate) const fn supports_top_two(self, plies: i32) -> bool {
        match self {
            Self::Gnubg => false,
            Self::NeonGammon => plies == 2,
        }
    }

    /// Deepest depth for practice answers, which stay quick: GNUbg 3-ply
    /// (its native 2-ply) or NeonGammon 2-ply.
    pub(crate) const fn practice_max_plies(self) -> i32 {
        match self {
            Self::Gnubg => 3,
            Self::NeonGammon => 2,
        }
    }

    /// Playing depth when a session selects this engine without a depth while
    /// the saved play depth belongs to another engine. `None` keeps the saved
    /// depth if this engine verifies it, else the shallowest verified depth.
    pub(crate) const fn play_plies_for_override(self) -> Option<i32> {
        match self {
            Self::Gnubg => None,
            Self::NeonGammon => Some(2),
        }
    }
}

/// User-facing short name for a stored engine ID.
pub(crate) fn display_name(engine_id: &str) -> Option<&'static str> {
    EngineId::parse(engine_id).map(EngineId::display_name)
}

#[cfg(test)]
mod tests {
    use super::EngineId;

    const ALL: [EngineId; 2] = [EngineId::Gnubg, EngineId::NeonGammon];

    #[test]
    fn stored_ids_round_trip() {
        for engine in ALL {
            assert_eq!(EngineId::parse(engine.id()), Some(engine));
        }
        assert_eq!(EngineId::NeonGammon.id(), "bgsage");
        assert_eq!(EngineId::parse("NeonGammon"), None);
        assert_eq!(EngineId::parse("GNUbg"), None);
        assert!(EngineId::require("other").is_err());
    }

    #[test]
    fn names_stay_user_facing() {
        assert_eq!(super::display_name("gnubg"), Some("GNUbg"));
        assert_eq!(super::display_name("bgsage"), Some("NeonGammon"));
        assert_eq!(super::display_name("xg"), None);
        assert_eq!(EngineId::Gnubg.product_name(), "GNU Backgammon");
    }

    #[test]
    fn every_engine_has_verified_depths_and_threads() {
        for engine in ALL {
            for purpose in ["analysis", "play", "duel"] {
                assert!(!engine.depths(purpose).is_empty(), "{engine:?} {purpose}");
            }
            assert!(engine.min_threads() >= 1);
            for purpose in ["analysis", "play"] {
                assert!(
                    engine.default_threads(purpose) >= engine.min_threads(),
                    "{engine:?} {purpose}"
                );
            }
            assert!(
                engine
                    .depths("analysis")
                    .contains(&engine.practice_max_plies())
            );
        }
        assert_eq!(EngineId::Gnubg.default_threads("analysis"), 1);
        assert_eq!(EngineId::Gnubg.default_threads("play"), 1);
        assert_eq!(EngineId::NeonGammon.default_threads("analysis"), 4);
        assert_eq!(EngineId::NeonGammon.default_threads("play"), 2);
    }

    #[test]
    fn capabilities_match_the_engines() {
        assert!(EngineId::Gnubg.is_rules_engine());
        assert!(!EngineId::NeonGammon.is_rules_engine());
        assert!(EngineId::NeonGammon.supports_top_two(2));
        assert!(!EngineId::NeonGammon.supports_top_two(3));
        assert!(!EngineId::Gnubg.supports_top_two(2));
        assert!(EngineId::NeonGammon.requires_verified_profile());
        assert!(!EngineId::Gnubg.has_difficulty_presets());
    }
}
