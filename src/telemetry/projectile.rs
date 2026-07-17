use avian3d::prelude::LinearVelocity;
use bevy::prelude::*;
use serde_json::{Value, json};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::robomaster::prelude::{Armor, ArmorRoot, ArmorSpec, SmallArmorLabel};

#[derive(Resource, Debug)]
pub struct ProjectileTelemetry {
    path: Option<PathBuf>,
    next_id: u64,
}

impl FromWorld for ProjectileTelemetry {
    fn from_world(_world: &mut World) -> Self {
        Self::from_env()
    }
}

impl ProjectileTelemetry {
    pub fn from_env() -> Self {
        let path = std::env::var_os("DAEDALUS_PROJECTILE_EVENTS_JSONL").map(PathBuf::from);
        Self { path, next_id: 1 }
    }

    #[cfg(test)]
    fn with_path(path: PathBuf) -> Self {
        Self {
            path: Some(path),
            next_id: 1,
        }
    }

    pub fn next_projectile_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn write_launch(&self, trace: &ProjectileTrace) {
        self.write(json!({
            "event": "launch",
            "trace": trace_json(trace),
        }));
    }

    pub fn write_armor_hit(
        &self,
        trace: Option<&ProjectileTrace>,
        projectile: ProjectileKinematics,
        target: ArmorTargetSnapshot,
    ) {
        self.write(json!({
            "event": "armor_hit",
            "trace": trace.map(trace_json),
            "projectile": projectile_json(projectile),
            "target": target_json(&target),
            "target_delta_m": target.position_m.map(|position| vec3_json(projectile.position_m - position)),
        }));
    }

    pub fn write_rune_hit(
        &self,
        trace: Option<&ProjectileTrace>,
        projectile: ProjectileKinematics,
        target: RuneTargetSnapshot,
        outcome: &str,
        accurate: bool,
        activates_rune: bool,
    ) {
        self.write(json!({
            "event": "rune_hit",
            "trace": trace.map(trace_json),
            "projectile": projectile_json(projectile),
            "target": rune_target_json(&target),
            "target_delta_m": target
                .target_position_m
                .map(|position| vec3_json(projectile.position_m - position)),
            "outcome": outcome,
            "accurate": accurate,
            "activates_rune": activates_rune,
        }));
    }

    pub fn write_expired(
        &self,
        trace: &ProjectileTrace,
        projectile: ProjectileKinematics,
        nearest_armor: Option<ArmorTargetSnapshot>,
        nearest_outpost_armor: Option<ArmorTargetSnapshot>,
    ) {
        self.write(json!({
            "event": "expired",
            "trace": trace_json(trace),
            "projectile": projectile_json(projectile),
            "nearest_armor": nearest_armor.as_ref().map(target_json),
            "nearest_delta_m": nearest_armor
                .and_then(|target| target.position_m)
                .map(|position| vec3_json(projectile.position_m - position)),
            "nearest_outpost_armor": nearest_outpost_armor.as_ref().map(target_json),
            "nearest_outpost_delta_m": nearest_outpost_armor
                .and_then(|target| target.position_m)
                .map(|position| vec3_json(projectile.position_m - position)),
        }));
    }

    fn write(&self, event: Value) {
        let Some(path) = &self.path else {
            return;
        };

        if let Err(error) = append_json_line(path, &event) {
            warn!(
                "Failed to write projectile telemetry to {}: {}",
                path.display(),
                error
            );
        }
    }
}

fn append_json_line(path: &Path, event: &Value) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    serde_json::to_writer(&mut file, event)?;
    file.write_all(b"\n")?;
    Ok(())
}

#[derive(Resource, Clone, Copy, Debug)]
pub struct PendingAutoAimShotContext {
    pub yaw_deg: Option<f32>,
    pub pitch_deg: Option<f32>,
    pub distance_m: Option<f32>,
    pub fire_advice: bool,
    pub aim_aligned: bool,
    pub actual_yaw_deg: f32,
    pub actual_pitch_deg: f32,
}

#[derive(Clone, Copy, Debug)]
pub enum ShotSource {
    Manual,
    AutoAim,
}

impl ShotSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::AutoAim => "auto_aim",
        }
    }
}

#[derive(Component, Clone, Debug)]
pub struct ProjectileTrace {
    pub id: u64,
    pub source: ShotSource,
    pub target_mode: String,
    pub launch_index: u32,
    pub launched_at_s: f64,
    pub position_m: Vec3,
    pub velocity_mps: Vec3,
    pub direction: Vec3,
    pub gimbal_yaw_deg: f32,
    pub gimbal_pitch_deg: f32,
    pub auto_aim: Option<PendingAutoAimShotContext>,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct ProjectileImpactRecorded;

#[derive(Clone, Copy, Debug)]
pub struct ProjectileKinematics {
    pub position_m: Vec3,
    pub velocity_mps: Option<Vec3>,
}

impl ProjectileKinematics {
    pub fn from_components(
        transform: Option<&Transform>,
        velocity: Option<&LinearVelocity>,
    ) -> Self {
        Self {
            position_m: transform.map_or(Vec3::ZERO, |transform| transform.translation),
            velocity_mps: velocity.map(|velocity| velocity.0),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ArmorTargetSnapshot {
    pub entity: Entity,
    pub name: String,
    pub team: String,
    pub spec: String,
    pub label: String,
    pub class: &'static str,
    pub position_m: Option<Vec3>,
}

impl ArmorTargetSnapshot {
    pub fn from_components(
        entity: Entity,
        armor: &Armor,
        transform: Option<&GlobalTransform>,
    ) -> Self {
        Self {
            entity,
            name: armor.name.clone(),
            team: format!("{:?}", armor.team),
            spec: format!("{:?}", armor.spec),
            label: format!("{:?}", armor.label),
            class: armor_target_class(armor.spec),
            position_m: transform.map(GlobalTransform::translation),
        }
    }
}

#[derive(Clone, Debug)]
pub struct RuneTargetSnapshot {
    pub rune: Entity,
    pub target_index: usize,
    pub team: String,
    pub mode: String,
    pub rune_position_m: Option<Vec3>,
    pub target_position_m: Option<Vec3>,
}

pub fn nearest_armor_target(
    position_m: Vec3,
    armors: &Query<(Entity, &Armor, &GlobalTransform), With<ArmorRoot>>,
) -> Option<ArmorTargetSnapshot> {
    armors
        .iter()
        .min_by(|(_, _, left_tf), (_, _, right_tf)| {
            let left_distance = left_tf.translation().distance_squared(position_m);
            let right_distance = right_tf.translation().distance_squared(position_m);
            left_distance.total_cmp(&right_distance)
        })
        .map(|(entity, armor, transform)| {
            ArmorTargetSnapshot::from_components(entity, armor, Some(transform))
        })
}

pub fn nearest_outpost_armor_target(
    position_m: Vec3,
    armors: &Query<(Entity, &Armor, &GlobalTransform), With<ArmorRoot>>,
) -> Option<ArmorTargetSnapshot> {
    armors
        .iter()
        .filter(|(_, armor, _)| armor_target_class(armor.spec) == "outpost")
        .min_by(|(_, _, left_tf), (_, _, right_tf)| {
            let left_distance = left_tf.translation().distance_squared(position_m);
            let right_distance = right_tf.translation().distance_squared(position_m);
            left_distance.total_cmp(&right_distance)
        })
        .map(|(entity, armor, transform)| {
            ArmorTargetSnapshot::from_components(entity, armor, Some(transform))
        })
}

fn armor_target_class(spec: ArmorSpec) -> &'static str {
    match spec {
        ArmorSpec::Small(SmallArmorLabel::Outpost) => "outpost",
        _ => "armor",
    }
}

fn trace_json(trace: &ProjectileTrace) -> Value {
    json!({
        "projectile_id": trace.id,
        "source": trace.source.as_str(),
        "target_mode": trace.target_mode,
        "launch_index": trace.launch_index,
        "launched_at_s": trace.launched_at_s,
        "launch_position_m": vec3_json(trace.position_m),
        "launch_velocity_mps": vec3_json(trace.velocity_mps),
        "launch_direction": vec3_json(trace.direction),
        "gimbal_yaw_deg": trace.gimbal_yaw_deg,
        "gimbal_pitch_deg": trace.gimbal_pitch_deg,
        "auto_aim": trace.auto_aim.map(auto_aim_context_json),
    })
}

fn auto_aim_context_json(context: PendingAutoAimShotContext) -> Value {
    json!({
        "yaw_deg": context.yaw_deg,
        "pitch_deg": context.pitch_deg,
        "distance_m": context.distance_m,
        "fire_advice": context.fire_advice,
        "aim_aligned": context.aim_aligned,
        "actual_yaw_deg": context.actual_yaw_deg,
        "actual_pitch_deg": context.actual_pitch_deg,
    })
}

fn projectile_json(projectile: ProjectileKinematics) -> Value {
    json!({
        "position_m": vec3_json(projectile.position_m),
        "velocity_mps": projectile.velocity_mps.map(vec3_json),
    })
}

fn target_json(target: &ArmorTargetSnapshot) -> Value {
    json!({
        "entity": format!("{:?}", target.entity),
        "name": target.name,
        "team": target.team,
        "spec": target.spec,
        "label": target.label,
        "class": target.class,
        "position_m": target.position_m.map(vec3_json),
    })
}

fn rune_target_json(target: &RuneTargetSnapshot) -> Value {
    json!({
        "rune": format!("{:?}", target.rune),
        "target_index": target.target_index,
        "team": target.team,
        "mode": target.mode,
        "rune_position_m": target.rune_position_m.map(vec3_json),
        "target_position_m": target.target_position_m.map(vec3_json),
    })
}

fn vec3_json(value: Vec3) -> Value {
    json!([value.x, value.y, value.z])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::robomaster::prelude::{ArmorLabel, Team};

    #[test]
    fn target_class_marks_outpost_separately() {
        assert_eq!(
            armor_target_class(ArmorSpec::Small(SmallArmorLabel::Outpost)),
            "outpost"
        );
        assert_eq!(
            armor_target_class(ArmorSpec::Small(SmallArmorLabel::InfantryTwo)),
            "armor"
        );
    }

    #[test]
    fn writes_launch_json_line_when_enabled() {
        let path = std::env::temp_dir().join(format!(
            "daedalus_projectile_telemetry_{}_{}.jsonl",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&path);

        let mut telemetry = ProjectileTelemetry::with_path(path.clone());
        let trace = ProjectileTrace {
            id: telemetry.next_projectile_id(),
            source: ShotSource::AutoAim,
            target_mode: "outpost".to_string(),
            launch_index: 1,
            launched_at_s: 1.25,
            position_m: Vec3::new(1.0, 2.0, 3.0),
            velocity_mps: Vec3::new(4.0, 5.0, 6.0),
            direction: Vec3::Y,
            gimbal_yaw_deg: 7.0,
            gimbal_pitch_deg: -8.0,
            auto_aim: Some(PendingAutoAimShotContext {
                yaw_deg: Some(7.0),
                pitch_deg: Some(82.0),
                distance_m: Some(4.5),
                fire_advice: true,
                aim_aligned: true,
                actual_yaw_deg: 7.0,
                actual_pitch_deg: -8.0,
            }),
        };

        telemetry.write_launch(&trace);
        let text = std::fs::read_to_string(&path).unwrap();
        let event: Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();

        assert_eq!(event["event"], "launch");
        assert_eq!(event["trace"]["projectile_id"], 1);
        assert_eq!(event["trace"]["source"], "auto_aim");
        assert_eq!(event["trace"]["target_mode"], "outpost");

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn armor_snapshot_keeps_target_identity() {
        let armor = Armor {
            name: "OUTPOST_ARMOR".to_string(),
            team: Team::Red,
            spec: ArmorSpec::Small(SmallArmorLabel::Outpost),
            label: ArmorLabel::OutpostZeo,
        };
        let snapshot = ArmorTargetSnapshot::from_components(Entity::PLACEHOLDER, &armor, None);

        assert_eq!(snapshot.name, "OUTPOST_ARMOR");
        assert_eq!(snapshot.class, "outpost");
        assert_eq!(snapshot.team, "Red");
    }

    #[test]
    fn writes_rune_hit_json_line_when_enabled() {
        let path = std::env::temp_dir().join(format!(
            "daedalus_projectile_rune_telemetry_{}_{}.jsonl",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&path);

        let mut telemetry = ProjectileTelemetry::with_path(path.clone());
        let trace = ProjectileTrace {
            id: telemetry.next_projectile_id(),
            source: ShotSource::AutoAim,
            target_mode: "big_buff".to_string(),
            launch_index: 1,
            launched_at_s: 1.25,
            position_m: Vec3::new(1.0, 2.0, 3.0),
            velocity_mps: Vec3::new(4.0, 5.0, 6.0),
            direction: Vec3::Y,
            gimbal_yaw_deg: 7.0,
            gimbal_pitch_deg: -8.0,
            auto_aim: None,
        };
        let target = RuneTargetSnapshot {
            rune: Entity::PLACEHOLDER,
            target_index: 2,
            team: "Red".to_string(),
            mode: "Large".to_string(),
            rune_position_m: Some(Vec3::new(7.0, 8.0, 9.0)),
            target_position_m: Some(Vec3::new(1.0, 2.5, 3.0)),
        };

        telemetry.write_rune_hit(
            Some(&trace),
            ProjectileKinematics {
                position_m: Vec3::new(1.0, 2.0, 3.0),
                velocity_mps: None,
            },
            target,
            "Activated",
            true,
            true,
        );
        let text = std::fs::read_to_string(&path).unwrap();
        let event: Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();

        assert_eq!(event["event"], "rune_hit");
        assert_eq!(event["trace"]["target_mode"], "big_buff");
        assert_eq!(event["target"]["target_index"], 2);
        assert_eq!(event["outcome"], "Activated");
        assert_eq!(event["accurate"], true);
        assert_eq!(event["activates_rune"], true);

        let _ = std::fs::remove_file(path);
    }
}
