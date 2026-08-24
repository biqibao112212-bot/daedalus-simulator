use bevy::prelude::Resource;

use crate::telemetry::ArmorTargetSnapshot;

/// The latest authoritative vehicle-armor score.  This is deliberately a
/// single read-only event rather than target truth: a participant can poll the
/// monotonically increasing event id after firing, but cannot enumerate
/// unhit armor or mutate the scoring state.
#[derive(Clone, Debug)]
pub(crate) struct ArmorHitEvent {
    pub event_id: u64,
    pub projectile_id: Option<u64>,
    pub target: ArmorTargetSnapshot,
    pub accurate_count: u32,
}

#[derive(Resource, Debug, Default)]
pub(crate) struct ArmorHitLedger {
    next_event_id: u64,
    latest: Option<ArmorHitEvent>,
}

impl ArmorHitLedger {
    pub fn record(
        &mut self,
        projectile_id: Option<u64>,
        target: ArmorTargetSnapshot,
        accurate_count: u32,
    ) {
        self.next_event_id = self.next_event_id.saturating_add(1).max(1);
        self.latest = Some(ArmorHitEvent {
            event_id: self.next_event_id,
            projectile_id,
            target,
            accurate_count,
        });
    }

    pub fn latest(&self) -> Option<&ArmorHitEvent> {
        self.latest.as_ref()
    }

    pub fn latest_event_id(&self) -> u64 {
        self.next_event_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::Entity;

    #[test]
    fn latest_hit_uses_a_monotonic_event_id() {
        let target = ArmorTargetSnapshot {
            entity: Entity::PLACEHOLDER,
            name: "INFANTRY_3".to_string(),
            team: "Red".to_string(),
            spec: "Small(InfantryThree)".to_string(),
            label: "InfantryThree".to_string(),
            class: "armor",
            position_m: None,
        };
        let mut ledger = ArmorHitLedger::default();
        ledger.record(Some(12), target.clone(), 1);
        ledger.record(None, target, 2);

        let latest = ledger.latest().expect("recorded hit");
        assert_eq!(ledger.latest_event_id(), 2);
        assert_eq!(latest.event_id, 2);
        assert_eq!(latest.projectile_id, None);
        assert_eq!(latest.accurate_count, 2);
        assert_eq!(latest.target.name, "INFANTRY_3");
    }
}
