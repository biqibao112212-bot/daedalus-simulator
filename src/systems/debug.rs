use bevy::prelude::*;
use bevy::render::view::screenshot::{Capturing, Screenshot, save_to_disk};
use bevy::window::{CursorIcon, SystemCursorIcon, Window};
use std::path::PathBuf;

use crate::components::{SlapperInfantry, SubscribeAutoAim};
use crate::config::SimulationConfig;
use crate::integrated_auto_aim::IntegratedAutoAimBridge;
use crate::robomaster::prelude::{Armor, ArmorStickerSelection};
use crate::statistic::ProjectileStatistics;
use crate::systems::FrequencyMetrics;

#[derive(Component)]
pub(crate) struct HelpText;

fn create_help_text(auto_aim: bool, bridge_status: &str, stats: &ProjectileStatistics) -> Text {
    format!(
        "auto-aim={} bridge={} total={} accurate={} pct={:.2}\nControls: F2-Screenshot F3-Camera F5-Auto Aim F6-Range Panel F7-Normal F8-Range F9-Energy F10-Small Rune F11-Large Rune F12-Close Rune | Space-Fire | WASD-Move Arrows/RMB-Gimbal",
        if auto_aim { "ON " } else { "OFF" },
        bridge_status,
        stats.launch_count,
        stats.accurate_count,
        stats.accurate_pct()
    )
        .into()
}

pub fn spawn_text(commands: &mut Commands) {
    commands.spawn((
        HelpText,
        Text::new(""),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(12.0),
            left: Val::Px(12.0),
            ..default()
        },
    ));
}

pub fn update_help_text(
    mut text: Query<&mut Text, With<HelpText>>,
    auto_aim: Res<SubscribeAutoAim>,
    bridge: Option<Res<IntegratedAutoAimBridge>>,
    stats: Res<ProjectileStatistics>,
) {
    let bridge_status = bridge
        .as_deref()
        .map(IntegratedAutoAimBridge::status_label)
        .unwrap_or("N/A");
    for mut text in text.iter_mut() {
        *text = create_help_text(
            auto_aim.load(std::sync::atomic::Ordering::Acquire),
            bridge_status,
            &stats,
        );
    }
}

pub fn export_debug_stats(
    time: Res<Time>,
    auto_aim: Res<SubscribeAutoAim>,
    bridge: Option<Res<IntegratedAutoAimBridge>>,
    stats: Res<ProjectileStatistics>,
    config: Option<Res<SimulationConfig>>,
    frequency: Option<Res<FrequencyMetrics>>,
    mut last_write_s: Local<f32>,
) {
    let Some(path) = std::env::var_os("DAEDALUS_STATS_JSON").map(PathBuf::from) else {
        return;
    };

    let now_s = time.elapsed_secs();
    if now_s - *last_write_s < 0.1 {
        return;
    }
    *last_write_s = now_s;

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let bridge_status = bridge
        .as_deref()
        .map(IntegratedAutoAimBridge::status_label)
        .unwrap_or("N/A");
    let auto_aim = auto_aim.load(std::sync::atomic::Ordering::Acquire);
    let frequency = frequency
        .as_deref()
        .map(FrequencyMetrics::snapshot)
        .unwrap_or_default();
    let (
        preview_enabled,
        preview_max_hz,
        physics_substeps,
        fixed_hz,
        physics_post_update,
        physics_tick_divisor,
    ) = config
        .as_deref()
        .map(|config| {
            (
                config.preview.enabled,
                config.preview.max_hz,
                config.physics.substep_count,
                config.physics.fixed_hz,
                config.physics.post_update,
                config.physics.tick_divisor,
            )
        })
        .unwrap_or((false, 0.0, 0, 0.0, false, 1));
    let json = format!(
        "{{\"elapsed_s\":{now_s:.3},\"auto_aim\":{},\"bridge\":\"{}\",\"launch_count\":{},\"accurate_count\":{},\"accurate_pct\":{:.6},\"preview_enabled\":{preview_enabled},\"preview_max_hz\":{preview_max_hz:.3},\"physics_substeps\":{physics_substeps},\"fixed_hz\":{fixed_hz:.3},\"physics_post_update\":{physics_post_update},\"physics_tick_divisor\":{physics_tick_divisor},\"talos_image_transport\":\"{}\",\"main_update_hz\":{:.3},\"render_fps\":{:.3},\"physics_step_hz\":{:.3},\"preview_present_hz\":{:.3},\"capture_copy_submit_hz\":{:.3},\"talos_publish_hz\":{:.3},\"talos_tcp_image_sent_hz\":{:.3},\"talos_frame_fps\":{:.3},\"network_command_receive_hz\":{:.3},\"sim_command_apply_hz\":{:.3},\"gimbal_update_hz\":{:.3},\"main_update_total\":{},\"physics_step_total\":{},\"preview_present_total\":{},\"capture_copy_submit_total\":{},\"capture_queue_drop_total\":{},\"capture_processing_complete_total\":{},\"capture_processing_in_flight\":{},\"capture_processing_max_in_flight\":{},\"capture_owned_rgba_attempt_total\":{},\"capture_owned_rgba_consumed_total\":{},\"capture_owned_rgba_fallback_total\":{},\"capture_borrowed_callback_total\":{},\"capture_fast_buffer_allocated\":{},\"capture_fast_buffer_allocation_total\":{},\"capture_fast_buffer_in_flight\":{},\"capture_fast_buffer_max_in_flight\":{},\"capture_fast_no_buffer_drop_total\":{},\"capture_fast_queue_pre_submit_drop_total\":{},\"capture_fast_map_callback_total\":{},\"capture_fast_map_success_total\":{},\"capture_fast_map_error_total\":{},\"capture_fast_mapped_copy_total\":{},\"capture_fast_map_callback_ns_total\":{},\"capture_fast_map_callback_ns_max\":{},\"capture_fast_submit_to_map_total\":{},\"capture_fast_submit_to_map_ns_total\":{},\"capture_fast_submit_to_map_ns_max\":{},\"capture_fast_mapped_copy_ns_total\":{},\"capture_fast_mapped_copy_ns_max\":{},\"talos_publish_total\":{},\"talos_publish_lock_drop_total\":{},\"talos_tcp_image_submit_total\":{},\"talos_tcp_image_sent_total\":{},\"talos_tcp_image_replaced_total\":{},\"talos_tcp_image_rejected_total\":{},\"talos_tcp_image_connect_total\":{},\"talos_tcp_image_disconnect_total\":{},\"talos_tcp_image_accept_fail_total\":{},\"talos_tcp_image_write_fail_total\":{},\"talos_tcp_image_write_timeout_total\":{},\"talos_tcp_image_write_connection_reset_total\":{},\"talos_tcp_image_write_broken_pipe_total\":{},\"talos_tcp_image_write_other_total\":{},\"talos_tcp_image_write_payload_bytes_before_error_total\":{},\"talos_tcp_image_last_write_error_kind\":\"{}\",\"talos_tcp_image_last_write_error_raw_os_code\":{},\"talos_tcp_image_last_write_payload_bytes_before_error\":{},\"talos_tcp_image_write_frame_duration_count\":{},\"talos_tcp_image_write_frame_duration_ns_total\":{},\"talos_tcp_image_write_frame_duration_ns_max\":{},\"talos_tcp_image_write_header_duration_count\":{},\"talos_tcp_image_write_header_duration_ns_total\":{},\"talos_tcp_image_write_header_duration_ns_max\":{},\"talos_tcp_image_write_payload_duration_count\":{},\"talos_tcp_image_write_payload_duration_ns_total\":{},\"talos_tcp_image_write_payload_duration_ns_max\":{},\"talos_tcp_image_write_would_block_total\":{},\"talos_tcp_image_write_retry_total\":{},\"talos_tcp_image_bind_fail_total\":{},\"talos_tcp_image_wire_bytes_sent_total\":{},\"talos_tcp_image_mailbox_current\":{},\"talos_tcp_image_mailbox_max\":{},\"talos_tcp_image_connected\":{},\"talos_tcp_image_latest_submitted_seq\":{},\"talos_tcp_image_latest_sent_seq\":{},\"talos_tcp_image_owned_submit_total\":{},\"talos_tcp_image_borrowed_submit_total\":{}}}",
        if auto_aim { "true" } else { "false" },
        bridge_status,
        stats.launch_count,
        stats.accurate_count,
        stats.accurate_pct(),
        frequency.talos_image_transport,
        frequency.main_update_hz,
        frequency.render_fps,
        frequency.physics_step_hz,
        frequency.preview_present_hz,
        frequency.capture_copy_submit_hz,
        frequency.talos_publish_hz,
        frequency.talos_tcp_image_sent_hz,
        frequency.talos_frame_fps,
        frequency.network_command_receive_hz,
        frequency.sim_command_apply_hz,
        frequency.gimbal_update_hz,
        frequency.main_update_total,
        frequency.physics_step_total,
        frequency.preview_present_total,
        frequency.capture_copy_submit_total,
        frequency.capture_queue_drop_total,
        frequency.capture_processing_complete_total,
        frequency.capture_processing_in_flight,
        frequency.capture_processing_max_in_flight,
        frequency.capture_owned_rgba_attempt_total,
        frequency.capture_owned_rgba_consumed_total,
        frequency.capture_owned_rgba_fallback_total,
        frequency.capture_borrowed_callback_total,
        frequency.capture_fast_buffer_allocated,
        frequency.capture_fast_buffer_allocation_total,
        frequency.capture_fast_buffer_in_flight,
        frequency.capture_fast_buffer_max_in_flight,
        frequency.capture_fast_no_buffer_drop_total,
        frequency.capture_fast_queue_pre_submit_drop_total,
        frequency.capture_fast_map_callback_total,
        frequency.capture_fast_map_success_total,
        frequency.capture_fast_map_error_total,
        frequency.capture_fast_mapped_copy_total,
        frequency.capture_fast_map_callback_ns_total,
        frequency.capture_fast_map_callback_ns_max,
        frequency.capture_fast_submit_to_map_total,
        frequency.capture_fast_submit_to_map_ns_total,
        frequency.capture_fast_submit_to_map_ns_max,
        frequency.capture_fast_mapped_copy_ns_total,
        frequency.capture_fast_mapped_copy_ns_max,
        frequency.talos_publish_total,
        frequency.talos_publish_lock_drop_total,
        frequency.talos_tcp_image_submit_total,
        frequency.talos_tcp_image_sent_total,
        frequency.talos_tcp_image_replaced_total,
        frequency.talos_tcp_image_rejected_total,
        frequency.talos_tcp_image_connect_total,
        frequency.talos_tcp_image_disconnect_total,
        frequency.talos_tcp_image_accept_fail_total,
        frequency.talos_tcp_image_write_fail_total,
        frequency.talos_tcp_image_write_timeout_total,
        frequency.talos_tcp_image_write_connection_reset_total,
        frequency.talos_tcp_image_write_broken_pipe_total,
        frequency.talos_tcp_image_write_other_total,
        frequency.talos_tcp_image_write_payload_bytes_before_error_total,
        frequency.talos_tcp_image_last_write_error_kind,
        frequency.talos_tcp_image_last_write_error_raw_os_code,
        frequency.talos_tcp_image_last_write_payload_bytes_before_error,
        frequency.talos_tcp_image_write_frame_duration_count,
        frequency.talos_tcp_image_write_frame_duration_ns_total,
        frequency.talos_tcp_image_write_frame_duration_ns_max,
        frequency.talos_tcp_image_write_header_duration_count,
        frequency.talos_tcp_image_write_header_duration_ns_total,
        frequency.talos_tcp_image_write_header_duration_ns_max,
        frequency.talos_tcp_image_write_payload_duration_count,
        frequency.talos_tcp_image_write_payload_duration_ns_total,
        frequency.talos_tcp_image_write_payload_duration_ns_max,
        frequency.talos_tcp_image_write_would_block_total,
        frequency.talos_tcp_image_write_retry_total,
        frequency.talos_tcp_image_bind_fail_total,
        frequency.talos_tcp_image_wire_bytes_sent_total,
        frequency.talos_tcp_image_mailbox_current,
        frequency.talos_tcp_image_mailbox_max,
        frequency.talos_tcp_image_connected,
        frequency.talos_tcp_image_latest_submitted_seq,
        frequency.talos_tcp_image_latest_sent_seq,
        frequency.talos_tcp_image_owned_submit_total,
        frequency.talos_tcp_image_borrowed_submit_total
    );
    let mut json = json;
    let _ = json.pop();
    let _ = std::fmt::Write::write_fmt(
        &mut json,
        format_args!(
            ",\"main_schedule_timing_total\":{},\"main_schedule_timing_ns_total\":{},\"main_schedule_timing_ns_max\":{},\"physics_step_ns_total\":{},\"physics_step_ns_max\":{}",
            frequency.main_schedule_timing_total,
            frequency.main_schedule_timing_ns_total,
            frequency.main_schedule_timing_ns_max,
            frequency.physics_step_ns_total,
            frequency.physics_step_ns_max,
        ),
    );
    json.push('}');
    let _ = std::fs::write(path, json);
}

pub fn change_appearance(
    keyboard: Res<ButtonInput<KeyCode>>,
    selections: Query<&mut ArmorStickerSelection, With<SlapperInfantry>>,
    owned: Query<&mut Armor, With<SlapperInfantry>>,
) {
    if keyboard.pressed(KeyCode::ShiftLeft) && keyboard.just_pressed(KeyCode::KeyC) {
        let mut n_type = None;
        for mut selection in selections {
            let new_typ = selection.advance_debug_sequence();
            n_type = Some(new_typ);
        }
        if let Some(n_type) = n_type {
            for mut own in owned {
                own.label = n_type;
            }
        }
    }
}

pub fn screenshot_on_f2(mut commands: Commands, mut counter: Local<u32>) {
    let path = format!("./screenshot-{}.png", *counter);
    *counter += 1;
    info!("Saving screenshot to {path}");
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path));
}

pub fn screenshot_saving(
    mut commands: Commands,
    screenshot_saving: Query<Entity, With<Capturing>>,
    window: Single<Entity, With<Window>>,
) {
    match screenshot_saving.iter().count() {
        0 => {
            commands.entity(*window).remove::<CursorIcon>();
        }
        x if x > 0 => {
            commands
                .entity(*window)
                .insert(CursorIcon::from(SystemCursorIcon::Progress));
        }
        _ => {}
    }
}
