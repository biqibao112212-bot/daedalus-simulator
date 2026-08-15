use crate::robomaster::common::Team;
use bevy::prelude::Resource;

pub const RUNE_EFFECTIVE_RADIUS_M: f32 = 0.150;
pub const RUNE_RING_COUNT: u8 = 10;

// Score a physical impact in the target plane. Ring 10 is the centre and
// ring 1 is the outermost 15 mm annulus of the 300 mm effective diameter.
pub fn ring_for_radius_m(radius_m: f32) -> u8 {
    let clamped = radius_m.max(0.0).min(RUNE_EFFECTIVE_RADIUS_M);
    let width = RUNE_EFFECTIVE_RADIUS_M / f32::from(RUNE_RING_COUNT);
    // An exact band boundary belongs to the next outer ring. The epsilon also
    // makes the 15 mm boundary stable across floating-point implementations.
    let from_centre = ((clamped + 0.000_001) / width).floor() as u8;
    RUNE_RING_COUNT.saturating_sub(from_centre).max(1)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BigRuneScore {
    run_id: u64,
    run_active: bool,
    activated_arms: u8,
    ring_total: u16,
    last_ring: u8,
    last_radius_mm: u16,
    last_target: i8,
}

#[derive(Resource, Debug, Clone, PartialEq, Eq, Default)]
pub struct BigRuneScores {
    red: BigRuneScore,
    blue: BigRuneScore,
}

impl BigRuneScores {
    pub fn for_team(&self, team: Team) -> &BigRuneScore {
        match team {
            Team::Red => &self.red,
            Team::Blue => &self.blue,
        }
    }

    pub fn for_team_mut(&mut self, team: Team) -> &mut BigRuneScore {
        match team {
            Team::Red => &mut self.red,
            Team::Blue => &mut self.blue,
        }
    }
}

impl Default for BigRuneScore {
    fn default() -> Self {
        Self {
            run_id: 0,
            run_active: false,
            activated_arms: 0,
            ring_total: 0,
            last_ring: 0,
            last_radius_mm: 0,
            last_target: -1,
        }
    }
}

impl BigRuneScore {
    pub fn begin_run_if_needed(&mut self, activating_large: bool) {
        if activating_large {
            if !self.run_active {
                self.run_id = self.run_id.saturating_add(1);
                self.activated_arms = 0;
                self.ring_total = 0;
                self.last_ring = 0;
                self.last_radius_mm = 0;
                self.last_target = -1;
                self.run_active = true;
            }
        } else {
            self.run_active = false;
        }
    }

    pub fn record_activated_arm(&mut self, target: usize, radius_m: f32) {
        let ring = ring_for_radius_m(radius_m);
        self.activated_arms = self.activated_arms.saturating_add(1);
        self.ring_total = self.ring_total.saturating_add(u16::from(ring));
        self.last_ring = ring;
        self.last_radius_mm = (radius_m.max(0.0) * 1000.0).round().min(u16::MAX as f32) as u16;
        self.last_target = i8::try_from(target).unwrap_or(-1);
    }

    pub const fn run_id(&self) -> u64 {
        self.run_id
    }

    pub const fn run_active(&self) -> bool {
        self.run_active
    }

    pub const fn activated_arms(&self) -> u8 {
        self.activated_arms
    }

    pub fn average_ring(&self) -> f32 {
        if self.activated_arms == 0 {
            0.0
        } else {
            f32::from(self.ring_total) / f32::from(self.activated_arms)
        }
    }

    pub const fn has_hit(&self) -> bool {
        self.activated_arms != 0
    }

    pub const fn last_ring(&self) -> u8 {
        self.last_ring
    }

    pub const fn last_radius_mm(&self) -> u16 {
        self.last_radius_mm
    }

    pub const fn last_target(&self) -> i8 {
        self.last_target
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_is_centre_high_and_edge_low() {
        assert_eq!(ring_for_radius_m(0.0), 10);
        assert_eq!(ring_for_radius_m(0.0149), 10);
        assert_eq!(ring_for_radius_m(0.015), 9);
        assert_eq!(ring_for_radius_m(0.149), 1);
        assert_eq!(ring_for_radius_m(0.150), 1);
    }

    #[test]
    fn score_tracks_all_activated_large_rune_arms() {
        let mut score = BigRuneScore::default();
        score.begin_run_if_needed(true);
        score.record_activated_arm(2, 0.0);
        score.record_activated_arm(3, 0.075);

        assert_eq!(score.activated_arms(), 2);
        assert_eq!(score.last_ring(), 5);
        assert_eq!(score.last_target(), 3);
        assert_eq!(score.average_ring(), 7.5);
        score.begin_run_if_needed(false);
        score.begin_run_if_needed(true);
        assert_eq!(score.activated_arms(), 0);
        assert_eq!(score.run_id(), 2);
    }
}
