//! Core 9-screen spatial topology solver, focus-follows-mouse router,
//! key-up failsafe watchdog, and O(1) per-OS key remapping engine.

use std::collections::{HashSet, VecDeque};
use spankvm_proto::{
    DeviceNode, GridPos, KeyMappingRule, OsFamily, OsKind, GRID_COLS, GRID_ROWS, MAX_DEVICES,
};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TopologyError {
    #[error("最多支持 {MAX_DEVICES} 台电脑互联，当前已达上限")]
    MaxDevicesReached,
    #[error("目标网格坐标超出 5×5 拓扑画布范围")]
    OutOfBounds,
    #[error("未找到指定的屏幕节点")]
    NodeNotFound,
    #[error("每台屏幕必须至少与一台其他屏幕保持上下左右正交相邻，以便鼠标跨屏移动")]
    DisconnectedIsland,
}

#[derive(Debug, Default, Clone)]
pub struct NeighborSet<'a> {
    pub up: Option<&'a DeviceNode>,
    pub down: Option<&'a DeviceNode>,
    pub left: Option<&'a DeviceNode>,
    pub right: Option<&'a DeviceNode>,
}

pub fn compute_neighbors<'a>(nodes: &'a [DeviceNode], node_id: &str) -> NeighborSet<'a> {
    let Some(origin) = nodes.iter().find(|n| n.id == node_id) else {
        return NeighborSet::default();
    };
    let GridPos { col, row } = origin.grid_pos;

    NeighborSet {
        up: nodes.iter().find(|n| n.grid_pos.col == col && n.grid_pos.row == row - 1),
        down: nodes.iter().find(|n| n.grid_pos.col == col && n.grid_pos.row == row + 1),
        left: nodes.iter().find(|n| n.grid_pos.col == col - 1 && n.grid_pos.row == row),
        right: nodes.iter().find(|n| n.grid_pos.col == col + 1 && n.grid_pos.row == row),
    }
}

pub fn is_topology_connected(nodes: &[DeviceNode]) -> bool {
    if nodes.len() <= 1 {
        return true;
    }

    let mut visited = HashSet::with_capacity(nodes.len());
    let mut queue = VecDeque::new();
    visited.insert(nodes[0].id.as_str());
    queue.push_back(&nodes[0]);

    while let Some(curr) = queue.pop_front() {
        for cand in nodes {
            if visited.contains(cand.id.as_str()) {
                continue;
            }
            let manhattan =
                (cand.grid_pos.col - curr.grid_pos.col).abs() + (cand.grid_pos.row - curr.grid_pos.row).abs();
            if manhattan == 1 {
                visited.insert(cand.id.as_str());
                queue.push_back(cand);
            }
        }
    }

    visited.len() == nodes.len()
}

pub fn set_primary_node(nodes: &mut [DeviceNode], primary_id: &str) -> Result<(), TopologyError> {
    if !nodes.iter().any(|n| n.id == primary_id) {
        return Err(TopologyError::NodeNotFound);
    }
    for n in nodes.iter_mut() {
        n.is_primary = n.id == primary_id;
    }
    Ok(())
}

pub fn move_node_to_cell(
    nodes: &[DeviceNode],
    node_id: &str,
    target_col: i32,
    target_row: i32,
) -> Result<Vec<DeviceNode>, TopologyError> {
    if !(0..GRID_COLS).contains(&target_col) || !(0..GRID_ROWS).contains(&target_row) {
        return Err(TopologyError::OutOfBounds);
    }

    let Some(moving_idx) = nodes.iter().position(|n| n.id == node_id) else {
        return Err(TopologyError::NodeNotFound);
    };
    let origin_pos = nodes[moving_idx].grid_pos;

    let mut next = nodes.to_vec();
    if let Some(occupant_idx) = next
        .iter()
        .position(|n| n.grid_pos.col == target_col && n.grid_pos.row == target_row)
    {
        next[occupant_idx].grid_pos = origin_pos;
    }
    next[moving_idx].grid_pos = GridPos {
        col: target_col,
        row: target_row,
    };

    if !is_topology_connected(&next) {
        return Err(TopologyError::DisconnectedIsland);
    }

    Ok(next)
}

/// Focus-Follows-Mouse state machine with key-up watchdog when crossing screen edges.
#[derive(Debug, Clone)]
pub struct FocusRouter {
    pub active_node_id: String,
    pub norm_x: f32,
    pub norm_y: f32,
    pub held_scancodes: HashSet<u16>,
    pub transition_count: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CursorStepOutcome {
    pub previous_node_id: String,
    pub active_node_id: String,
    pub norm_x: f32,
    pub norm_y: f32,
    /// Scancodes that must receive immediate synthetic KeyUp on `previous_node_id`
    /// so modifier keys never stick when focus follows mouse to another computer.
    pub synthetic_keyups_for_previous: Vec<u16>,
}

impl FocusRouter {
    pub fn new(primary_id: impl Into<String>) -> Self {
        Self {
            active_node_id: primary_id.into(),
            norm_x: 0.5,
            norm_y: 0.5,
            held_scancodes: HashSet::new(),
            transition_count: 0,
        }
    }

    pub fn record_key_state(&mut self, scancode: u16, pressed: bool) {
        if pressed {
            self.held_scancodes.insert(scancode);
        } else {
            self.held_scancodes.remove(&scancode);
        }
    }

    pub fn force_return_to_primary(&mut self, primary_id: &str) -> CursorStepOutcome {
        let prev = self.active_node_id.clone();
        let released: Vec<u16> = self.held_scancodes.drain().collect();
        self.active_node_id = primary_id.to_string();
        self.norm_x = 0.5;
        self.norm_y = 0.5;
        CursorStepOutcome {
            previous_node_id: prev,
            active_node_id: self.active_node_id.clone(),
            norm_x: 0.5,
            norm_y: 0.5,
            synthetic_keyups_for_previous: released,
        }
    }

    pub fn step_pointer_delta(
        &mut self,
        nodes: &[DeviceNode],
        dx_px: f32,
        dy_px: f32,
    ) -> CursorStepOutcome {
        let prev_id = self.active_node_id.clone();
        let Some(active) = nodes.iter().find(|n| n.id == self.active_node_id) else {
            return CursorStepOutcome {
                previous_node_id: prev_id.clone(),
                active_node_id: prev_id,
                norm_x: self.norm_x,
                norm_y: self.norm_y,
                synthetic_keyups_for_previous: Vec::new(),
            };
        };

        let w = active.resolution.width.max(1) as f32;
        let h = active.resolution.height.max(1) as f32;
        let mut nx = self.norm_x + (dx_px / w);
        let mut ny = self.norm_y + (dy_px / h);
        let mut target_id = active.id.clone();

        let neighbors = compute_neighbors(nodes, &active.id);
        if nx < 0.0 {
            if let Some(left) = neighbors.left.filter(|n| n.online) {
                target_id = left.id.clone();
                nx = (1.0 + nx).clamp(0.02, 0.98);
            } else {
                nx = 0.0;
            }
        } else if nx > 1.0 {
            if let Some(right) = neighbors.right.filter(|n| n.online) {
                target_id = right.id.clone();
                nx = (nx - 1.0).clamp(0.02, 0.98);
            } else {
                nx = 1.0;
            }
        }

        let v_neighbors = compute_neighbors(nodes, &target_id);
        if ny < 0.0 {
            if let Some(up) = v_neighbors.up.filter(|n| n.online) {
                target_id = up.id.clone();
                ny = (1.0 + ny).clamp(0.02, 0.98);
            } else {
                ny = 0.0;
            }
        } else if ny > 1.0 {
            if let Some(down) = v_neighbors.down.filter(|n| n.online) {
                target_id = down.id.clone();
                ny = (ny - 1.0).clamp(0.02, 0.98);
            } else {
                ny = 1.0;
            }
        }

        let mut released = Vec::new();
        if target_id != prev_id {
            self.transition_count += 1;
            released = self.held_scancodes.drain().collect();
        }

        self.active_node_id = target_id.clone();
        self.norm_x = nx.clamp(0.0, 1.0);
        self.norm_y = ny.clamp(0.0, 1.0);

        CursorStepOutcome {
            previous_node_id: prev_id,
            active_node_id: target_id,
            norm_x: self.norm_x,
            norm_y: self.norm_y,
            synthetic_keyups_for_previous: released,
        }
    }
}

pub fn resolve_mapped_key<'a>(
    rules: &'a [KeyMappingRule],
    physical_key: &'a str,
    target_os: OsKind,
) -> &'a str {
    let family_tag = match target_os.family() {
        OsFamily::MacOs => "macos",
        OsFamily::Windows => "windows",
        OsFamily::Linux => "linux",
    };

    if let Some(specific) = rules.iter().find(|r| {
        r.enabled
            && r.target_os.eq_ignore_ascii_case(family_tag)
            && r.source_key.eq_ignore_ascii_case(physical_key)
    }) {
        return specific.target_key.as_str();
    }

    if let Some(global) = rules.iter().find(|r| {
        r.enabled
            && r.target_os.eq_ignore_ascii_case("all")
            && r.source_key.eq_ignore_ascii_case(physical_key)
    }) {
        return global.target_key.as_str();
    }

    physical_key
}
