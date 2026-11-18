use std::sync::RwLock;
use serde::Serialize;
use spankvm_core::{move_node_to_cell, set_primary_node, FocusRouter};
use spankvm_pal::detect_active_driver_stack;
use spankvm_proto::{ClipboardPolicy, DeviceNode, EngineTuning, KeyMappingRule, MAX_DEVICES};
use tauri::State;

pub struct SharedDaemonState {
    pub nodes: Vec<DeviceNode>,
    pub router: FocusRouter,
    pub key_rules: Vec<KeyMappingRule>,
    pub tuning: EngineTuning,
    pub clipboard_policy: ClipboardPolicy,
    pub local_ephemeral_pin: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaemonSnapshotResponse {
    pub nodes: Vec<DeviceNode>,
    pub active_node_id: String,
    pub cursor_x: f32,
    pub cursor_y: f32,
    pub transition_count: u64,
    pub key_rules: Vec<KeyMappingRule>,
    pub tuning: EngineTuning,
    pub clipboard_policy: ClipboardPolicy,
    pub local_ephemeral_pin: String,
    pub active_driver_name: String,
    pub wayland_active: bool,
}

#[tauri::command]
pub fn get_daemon_snapshot(
    state: State<'_, RwLock<SharedDaemonState>>,
) -> Result<DaemonSnapshotResponse, String> {
    let guard = state.read().map_err(|e| e.to_string())?;
    let driver = detect_active_driver_stack(guard.tuning.linux_wayland_strategy);
    Ok(DaemonSnapshotResponse {
        nodes: guard.nodes.clone(),
        active_node_id: guard.router.active_node_id.clone(),
        cursor_x: guard.router.norm_x,
        cursor_y: guard.router.norm_y,
        transition_count: guard.router.transition_count,
        key_rules: guard.key_rules.clone(),
        tuning: guard.tuning.clone(),
        clipboard_policy: guard.clipboard_policy.clone(),
        local_ephemeral_pin: guard.local_ephemeral_pin.clone(),
        active_driver_name: driver.os_backend_name.to_string(),
        wayland_active: driver.wayland_active,
    })
}

#[tauri::command]
pub fn cmd_set_primary_node(
    primary_id: String,
    state: State<'_, RwLock<SharedDaemonState>>,
) -> Result<Vec<DeviceNode>, String> {
    let mut guard = state.write().map_err(|e| e.to_string())?;
    set_primary_node(&mut guard.nodes, &primary_id).map_err(|e| e.to_string())?;
    Ok(guard.nodes.clone())
}

#[tauri::command]
pub fn cmd_move_node_cell(
    node_id: String,
    col: i32,
    row: i32,
    state: State<'_, RwLock<SharedDaemonState>>,
) -> Result<Vec<DeviceNode>, String> {
    let mut guard = state.write().map_err(|e| e.to_string())?;
    let updated = move_node_to_cell(&guard.nodes, &node_id, col, row).map_err(|e| e.to_string())?;
    guard.nodes = updated.clone();
    Ok(updated)
}

#[tauri::command]
pub fn cmd_emergency_return_primary(
    state: State<'_, RwLock<SharedDaemonState>>,
) -> Result<String, String> {
    let mut guard = state.write().map_err(|e| e.to_string())?;
    let primary_id = guard
        .nodes
        .iter()
        .find(|n| n.is_primary)
        .map(|n| n.id.clone())
        .unwrap_or_default();
    let outcome = guard.router.force_return_to_primary(&primary_id);
    Ok(outcome.active_node_id)
}

#[tauri::command]
pub fn cmd_update_engine_tuning(
    tuning: EngineTuning,
    state: State<'_, RwLock<SharedDaemonState>>,
) -> Result<EngineTuning, String> {
    let mut guard = state.write().map_err(|e| e.to_string())?;
    guard.tuning = tuning.clone();
    Ok(tuning)
}

#[tauri::command]
pub fn cmd_check_capacity(
    state: State<'_, RwLock<SharedDaemonState>>,
) -> Result<(usize, usize), String> {
    let guard = state.read().map_err(|e| e.to_string())?;
    Ok((guard.nodes.len(), MAX_DEVICES))
}
