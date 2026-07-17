use crate::robomaster::power_rune::common::{RUNE_TARGET_COUNT, RuneMode};
use crate::robomaster::power_rune::rotation::PowerRuneRotation;
use crate::robomaster::power_rune::state::MechanismState;
use crate::robomaster::power_rune::visual::PowerRuneVisuals;
use crate::robomaster::prelude::Team;
use crate::robomaster::visibility::StatefulAppearance;
use bevy::app::Update;
use bevy::prelude::{
    ButtonInput, Component, IntoScheduleConfigs, KeyCode, Query, Res, ResMut, Resource, Time,
    Transform,
};

const MANUAL_INACTIVE_HOLD_SECS: f32 = 60.0 * 60.0;

#[derive(Component, Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub struct PowerRune {
    team: Team,
    mode: RuneMode,
}

#[derive(Component, Debug, Clone, PartialEq)]
pub struct PowerRuneMechanism {
    state: MechanismState,
}

#[derive(Resource, Debug, Default, Copy, Clone, PartialEq, Eq)]
pub struct ManualPowerRuneControlState {
    override_enabled: bool,
    active_mode: Option<RuneMode>,
}

#[derive(Resource, Debug, Default, Copy, Clone, PartialEq, Eq)]
struct InitialPowerRuneControlApplied(bool);

impl PowerRune {
    pub fn new(team: Team, mode: RuneMode) -> Self {
        Self { team, mode }
    }

    pub fn team(&self) -> Team {
        self.team
    }

    pub fn mode(&self) -> RuneMode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: RuneMode) {
        self.mode = mode;
    }
}

impl PowerRuneMechanism {
    pub fn new(mode: RuneMode) -> Self {
        Self {
            state: MechanismState::inactive(mode),
        }
    }

    pub fn state(&self) -> &MechanismState {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut MechanismState {
        &mut self.state
    }
}

impl Default for PowerRuneMechanism {
    fn default() -> Self {
        Self::new(RuneMode::Small)
    }
}

fn held_inactive_state(mode: RuneMode) -> MechanismState {
    MechanismState::Inactive {
        mode,
        remaining: MANUAL_INACTIVE_HOLD_SECS,
    }
}

fn requested_rune_mode_from_env() -> Option<Option<RuneMode>> {
    let mode = std::env::var("DAEDALUS_RUNE_MODE").ok()?;
    match mode.trim().to_ascii_lowercase().as_str() {
        "small" | "small_buff" | "small-buff" | "small_rune" | "small-rune" => {
            Some(Some(RuneMode::Small))
        }
        "large" | "big" | "big_buff" | "big-buff" | "large_rune" | "large-rune" | "big_rune"
        | "big-rune" => Some(Some(RuneMode::Large)),
        "closed" | "close" | "inactive" | "off" => Some(None),
        _ => None,
    }
}

fn parse_rune_target_indices(raw: &str) -> Vec<usize> {
    let mut targets = raw
        .split(',')
        .filter_map(|part| part.trim().parse::<usize>().ok())
        .filter(|target| *target < RUNE_TARGET_COUNT)
        .collect::<Vec<_>>();
    targets.sort_unstable();
    targets.dedup();
    targets
}

fn rune_target_indices_from_env(key: &str) -> Option<Vec<usize>> {
    std::env::var(key)
        .ok()
        .map(|raw| parse_rune_target_indices(&raw))
}

fn forced_power_rune_state_from_env(mode: RuneMode) -> Option<MechanismState> {
    let state = std::env::var("DAEDALUS_RUNE_STATE")
        .ok()
        .map(|value| value.trim().to_ascii_lowercase());
    if matches!(
        state.as_deref(),
        Some("activated" | "complete" | "completed" | "all_activated" | "all-activated")
    ) {
        return Some(MechanismState::forced_activated(mode));
    }

    let pending = rune_target_indices_from_env("DAEDALUS_RUNE_PENDING_TARGETS")
        .or_else(|| rune_target_indices_from_env("DAEDALUS_RUNE_TARGETS"));
    let activated = rune_target_indices_from_env("DAEDALUS_RUNE_ACTIVATED_TARGETS");
    if pending.is_none() && activated.is_none() {
        return None;
    }

    Some(MechanismState::forced_activation_state(
        mode,
        pending.as_deref().unwrap_or(&[]),
        activated.as_deref().unwrap_or(&[]),
    ))
}

fn initial_power_rune_state(mode: RuneMode, rng: &mut impl rand::Rng) -> MechanismState {
    forced_power_rune_state_from_env(mode).unwrap_or_else(|| MechanismState::start(mode, rng))
}

fn apply_power_rune_control(
    active_mode: Option<RuneMode>,
    control: &mut ManualPowerRuneControlState,
    runes: &mut Query<(&mut PowerRune, &mut PowerRuneMechanism)>,
) -> bool {
    let mut applied = false;
    let mut rng = rand::thread_rng();
    for (mut rune, mut mechanism) in runes.iter_mut() {
        if let Some(mode) = active_mode {
            rune.set_mode(mode);
            *mechanism.state_mut() = initial_power_rune_state(mode, &mut rng);
        } else {
            *mechanism.state_mut() = held_inactive_state(rune.mode());
        }
        applied = true;
    }

    if applied {
        control.override_enabled = true;
        control.active_mode = active_mode;
    }

    applied
}

fn initial_power_rune_control_from_env(
    mut applied: ResMut<InitialPowerRuneControlApplied>,
    mut control: ResMut<ManualPowerRuneControlState>,
    mut runes: Query<(&mut PowerRune, &mut PowerRuneMechanism)>,
) {
    if applied.0 {
        return;
    }
    let Some(active_mode) = requested_rune_mode_from_env() else {
        applied.0 = true;
        return;
    };

    if apply_power_rune_control(active_mode, &mut control, &mut runes) {
        applied.0 = true;
    }
}

#[cfg(test)]
mod tests {
    use super::parse_rune_target_indices;

    #[test]
    fn parses_rune_target_indices_with_bounds_and_dedup() {
        assert_eq!(
            parse_rune_target_indices("4, 1, 9, 1, bad, 0"),
            vec![0, 1, 4]
        );
    }
}

fn manual_power_rune_controls(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut control: ResMut<ManualPowerRuneControlState>,
    mut runes: Query<(&mut PowerRune, &mut PowerRuneMechanism)>,
) {
    let requested_mode = if keyboard.just_pressed(KeyCode::F10) {
        Some(Some(RuneMode::Small))
    } else if keyboard.just_pressed(KeyCode::F11) {
        Some(Some(RuneMode::Large))
    } else if keyboard.just_pressed(KeyCode::F12) {
        Some(None)
    } else {
        None
    };
    let Some(active_mode) = requested_mode else {
        return;
    };

    apply_power_rune_control(active_mode, &mut control, &mut runes);
}

fn enforce_manual_power_rune_close(
    control: Res<ManualPowerRuneControlState>,
    mut runes: Query<(&PowerRune, &mut PowerRuneMechanism)>,
) {
    if !control.override_enabled || control.active_mode.is_some() {
        return;
    }

    for (rune, mut mechanism) in &mut runes {
        *mechanism.state_mut() = held_inactive_state(rune.mode());
    }
}

fn rune_activation_tick(
    time: Res<Time>,
    mut runes: Query<(&mut PowerRuneMechanism, &mut PowerRuneRotation)>,
) {
    let delta_secs = time.delta_secs();
    let mut rng = rand::thread_rng();

    for (mut mechanism, mut rotation) in &mut runes {
        mechanism.state.tick(delta_secs, &mut rng);
        rotation.sync_activation(
            mechanism.state.mode(),
            mechanism.state.is_activating(),
            &mut rng,
        );
    }
}

fn apply_power_rune_visuals(
    mut runes: Query<(&PowerRune, &PowerRuneMechanism, &mut PowerRuneVisuals)>,
    mut appearance: StatefulAppearance,
) {
    for (rune, mechanism, mut visuals) in &mut runes {
        visuals.apply(rune.mode(), mechanism.state(), &mut appearance);
    }
}

fn rune_rotation_system(
    time: Res<Time>,
    mut runes: Query<(&PowerRune, &mut PowerRuneRotation, &mut Transform)>,
) {
    let dt = time.delta_secs();
    for (rune, mut rotation, mut transform) in &mut runes {
        rotation.rotate(rune.mode(), &mut transform, dt);
    }
}

#[derive(Default)]
pub(super) struct PowerRuneUpdatePlugin;

impl bevy::app::Plugin for PowerRuneUpdatePlugin {
    fn build(&self, app: &mut bevy::app::App) {
        app.init_resource::<ManualPowerRuneControlState>()
            .init_resource::<InitialPowerRuneControlApplied>()
            .add_systems(
                Update,
                (
                    initial_power_rune_control_from_env,
                    manual_power_rune_controls,
                    enforce_manual_power_rune_close,
                    rune_activation_tick,
                    apply_power_rune_visuals,
                    rune_rotation_system,
                )
                    .chain(),
            );
    }
}
