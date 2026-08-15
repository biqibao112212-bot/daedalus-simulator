use crate::robomaster::power_rune::common::{RUNE_TARGET_COUNT, RuneMode};
use crate::robomaster::power_rune::rotation::PowerRuneRotation;
use crate::robomaster::power_rune::score::BigRuneScores;
use crate::robomaster::power_rune::state::{MechanismState, RuneTargetStates};
use crate::robomaster::power_rune::visual::PowerRuneVisuals;
use crate::robomaster::prelude::Team;
use crate::robomaster::visibility::StatefulAppearance;
use crate::setup::{AutoAimSceneMode, AutoAimSceneState};
use bevy::app::Update;
use bevy::log::info;
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
    visual_override: Option<RuneTargetStates>,
    rotation_paused: bool,
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
    runes: &mut Query<(
        &mut PowerRune,
        &mut PowerRuneMechanism,
        &mut PowerRuneRotation,
    )>,
) -> bool {
    let mut applied = false;
    let mut rng = rand::thread_rng();
    for (mut rune, mut mechanism, mut rotation) in runes.iter_mut() {
        if let Some(mode) = active_mode {
            rune.set_mode(mode);
            *mechanism.state_mut() = initial_power_rune_state(mode, &mut rng);
        } else {
            *mechanism.state_mut() = held_inactive_state(rune.mode());
        }
        rotation.set_paused(false);
        applied = true;
    }

    if applied {
        control.override_enabled = true;
        control.active_mode = active_mode;
        control.visual_override = None;
        control.rotation_paused = false;
    }

    applied
}

pub(crate) fn apply_scene_control_power_rune_state(
    active_mode: Option<RuneMode>,
    pending_targets: &[usize],
    activated_targets: &[usize],
    control: &mut ManualPowerRuneControlState,
    runes: &mut Query<(
        &mut PowerRune,
        &mut PowerRuneMechanism,
        &mut PowerRuneRotation,
    )>,
) -> bool {
    let mut applied = false;
    for (mut rune, mut mechanism, mut rotation) in runes.iter_mut() {
        if let Some(mode) = active_mode {
            rune.set_mode(mode);
            *mechanism.state_mut() = if activated_targets.len() == RUNE_TARGET_COUNT {
                MechanismState::forced_activated(mode)
            } else {
                MechanismState::forced_activation_state(mode, pending_targets, activated_targets)
            };
        } else {
            *mechanism.state_mut() = held_inactive_state(rune.mode());
        }
        rotation.set_paused(false);
        applied = true;
    }
    if applied {
        control.override_enabled = true;
        control.active_mode = active_mode;
        control.visual_override = None;
        control.rotation_paused = false;
    }
    applied
}

pub(crate) fn apply_scene_control_power_rune_scenario(
    mode: RuneMode,
    rule_driven: bool,
    red_face_clockwise: bool,
    leaf_states: Option<RuneTargetStates>,
    control: &mut ManualPowerRuneControlState,
    runes: &mut Query<(
        &mut PowerRune,
        &mut PowerRuneMechanism,
        &mut PowerRuneRotation,
    )>,
) -> bool {
    let mut applied = false;
    let mut rng = rand::thread_rng();
    for (mut rune, mut mechanism, mut rotation) in runes.iter_mut() {
        rune.set_mode(mode);
        rotation.set_clockwise(rotation_is_clockwise_for_team(
            rune.team(),
            red_face_clockwise,
        ));
        rotation.set_paused(!rule_driven);
        *mechanism.state_mut() = if rule_driven {
            MechanismState::start(mode, &mut rng)
        } else {
            held_inactive_state(mode)
        };
        applied = true;
    }
    if applied {
        control.override_enabled = true;
        control.active_mode = Some(mode);
        control.visual_override = leaf_states;
        control.rotation_paused = !rule_driven;
    }
    applied
}

fn initial_power_rune_control_from_env(
    mut applied: ResMut<InitialPowerRuneControlApplied>,
    mut control: ResMut<ManualPowerRuneControlState>,
    mut runes: Query<(
        &mut PowerRune,
        &mut PowerRuneMechanism,
        &mut PowerRuneRotation,
    )>,
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
    mut runes: Query<(
        &mut PowerRune,
        &mut PowerRuneMechanism,
        &mut PowerRuneRotation,
    )>,
) {
    let requested_mode = if keyboard.just_pressed(KeyCode::F10) {
        Some(Some(if crate::distribution::is_contest_release() {
            RuneMode::Large
        } else {
            RuneMode::Small
        }))
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

fn rotation_is_clockwise_for_team(team: Team, requested_clockwise: bool) -> bool {
    match team {
        Team::Red => requested_clockwise,
        Team::Blue => !requested_clockwise,
    }
}

fn contest_power_rune_mode_controls(
    keyboard: Res<ButtonInput<KeyCode>>,
    scene_state: Res<AutoAimSceneState>,
    mut control: ResMut<ManualPowerRuneControlState>,
    mut runes: Query<(
        &mut PowerRune,
        &mut PowerRuneMechanism,
        &mut PowerRuneRotation,
    )>,
) {
    if !crate::distribution::is_contest_release() || scene_state.current != AutoAimSceneMode::Energy
    {
        return;
    }

    let requested_mode = if keyboard.just_pressed(KeyCode::KeyQ) {
        Some(RuneMode::Small)
    } else if keyboard.just_pressed(KeyCode::KeyE) {
        Some(RuneMode::Large)
    } else {
        None
    };
    let Some(requested_mode) = requested_mode else {
        return;
    };
    apply_power_rune_control(Some(requested_mode), &mut control, &mut runes);
    info!("Contest energy mechanism switched to {requested_mode:?} mode.");
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
    control: Res<ManualPowerRuneControlState>,
    mut runes: Query<(&mut PowerRuneMechanism, &mut PowerRuneRotation)>,
) {
    if control.rotation_paused {
        return;
    }
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

fn track_big_rune_score_runs(
    runes: Query<(&PowerRune, &PowerRuneMechanism)>,
    mut scores: ResMut<BigRuneScores>,
) {
    for (rune, mechanism) in &runes {
        scores
            .for_team_mut(rune.team())
            .begin_run_if_needed(mechanism.state().is_activating_large());
    }
}

fn apply_power_rune_visuals(
    control: Res<ManualPowerRuneControlState>,
    mut runes: Query<(&PowerRune, &PowerRuneMechanism, &mut PowerRuneVisuals)>,
    mut appearance: StatefulAppearance,
) {
    for (rune, mechanism, mut visuals) in &mut runes {
        if let Some(states) = control.visual_override {
            visuals.apply_target_states(rune.mode(), &states, &mut appearance);
        } else {
            visuals.apply(rune.mode(), mechanism.state(), &mut appearance);
        }
    }
}

fn rune_rotation_system(
    time: Res<Time>,
    control: Res<ManualPowerRuneControlState>,
    mut runes: Query<(&PowerRune, &mut PowerRuneRotation, &mut Transform)>,
) {
    if control.rotation_paused {
        return;
    }
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
                    contest_power_rune_mode_controls,
                    enforce_manual_power_rune_close,
                    rune_activation_tick,
                    track_big_rune_score_runs,
                    apply_power_rune_visuals,
                    rune_rotation_system,
                )
                    .chain(),
            );
    }
}

#[cfg(test)]
mod rotation_control_tests {
    use super::*;

    #[test]
    fn opposite_faces_keep_opposite_directions() {
        assert!(rotation_is_clockwise_for_team(Team::Red, true));
        assert!(!rotation_is_clockwise_for_team(Team::Blue, true));
        assert!(!rotation_is_clockwise_for_team(Team::Red, false));
        assert!(rotation_is_clockwise_for_team(Team::Blue, false));
    }
}
