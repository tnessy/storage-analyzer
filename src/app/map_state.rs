//! Treemap state: scope, cached layout, throttling and map-focus navigation (T017).

use std::time::Duration;

use ratatui::layout::Rect;

use super::tree_state::TreeState;
use crate::layout::{Direction, Tile, neighbor, squarify};
use crate::model::Model;
use crate::types::{Kind, NodeId, SizeMode};
use crate::ui::theme::Theme;
use crate::ui::tile_colors::{Fill, TileColors, TileKind, adjacency, assign};

/// While scanning, the layout is recomputed at most this often (design §5: ≤ 2 Hz).
pub const RELAYOUT_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pane {
    Tree,
    Map,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Key {
    scope: NodeId,
    area: Rect,
    mode: SizeMode,
    aspect_bits: u32,
}

#[derive(Debug)]
struct Cache {
    key: Key,
    /// What the scope looked like when the layout was made: its size and child count.
    fingerprint: (u64, usize),
    at: Duration,
    tiles: Vec<Tile>,
    /// For each tile, its colour (UX §7, option C).
    fills: Vec<Fill>,
    /// Items with a size that got no tile of their own, with their sizes.
    hidden: Vec<(NodeId, u64)>,
    child_count: usize,
    /// Sum of all children's sizes.
    total: u64,
}

#[derive(Debug, Default)]
pub struct MapState {
    /// The cursor is on the `(+N smaller)` tile, reached from the map; the tree keeps its selection.
    pub on_aggregate: bool,
    cache: Option<Cache>,
    relayouts: u64,
}

/// Which folder's children the map shows (design §3.7, UX §3.3).
pub fn scope_of(model: &Model, tree: &TreeState) -> NodeId {
    if let Some(dir) = tree.aggregate {
        dir
    } else if tree.selection == tree.view_root {
        tree.view_root
    } else {
        model.parent(tree.selection).unwrap_or(tree.view_root)
    }
}

impl MapState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tiles(&self) -> &[Tile] {
        self.cache.as_ref().map_or(&[], |c| &c.tiles)
    }

    pub fn relayouts(&self) -> u64 {
        self.relayouts
    }

    pub fn scope(&self) -> Option<NodeId> {
        self.cache.as_ref().map(|c| c.key.scope)
    }

    /// The colour of each tile, parallel to `tiles()`.
    pub fn fills(&self) -> &[Fill] {
        self.cache.as_ref().map_or(&[], |c| &c.fills)
    }

    /// Items with a size that have no tile of their own (they are in the aggregate tile, or lost).
    pub fn hidden(&self) -> &[(NodeId, u64)] {
        self.cache.as_ref().map_or(&[], |c| &c.hidden)
    }

    pub fn child_count(&self) -> usize {
        self.cache.as_ref().map_or(0, |c| c.child_count)
    }

    /// Sum of the scope's children's sizes.
    pub fn total(&self) -> u64 {
        self.cache.as_ref().map_or(0, |c| c.total)
    }

    /// The index of the highlighted tile, if any.
    pub fn selected_index(&self, tree: &TreeState) -> Option<usize> {
        let tiles = self.tiles();
        if tree.on_parent {
            None
        } else if tree.aggregate.is_some() || self.on_aggregate {
            tiles.iter().position(|t| t.id.is_none())
        } else {
            tiles.iter().position(|t| t.id == Some(tree.selection))
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        &mut self,
        model: &Model,
        tree: &TreeState,
        mode: SizeMode,
        aspect: f32,
        area: Rect,
        now: Duration,
        scanning: bool,
        theme: &Theme,
        colors: &mut TileColors,
    ) {
        let scope = scope_of(model, tree);
        let key = Key {
            scope,
            area,
            mode,
            aspect_bits: aspect.to_bits(),
        };
        let fingerprint = (model.size(scope, mode), model.children(scope).len());
        let stale = match &self.cache {
            None => true,
            Some(c) if c.key != key => true,
            Some(c) => {
                c.fingerprint != fingerprint
                    && (!scanning || now.saturating_sub(c.at) >= RELAYOUT_INTERVAL)
            }
        };
        if !stale {
            return;
        }
        let items: Vec<(NodeId, u64)> = model
            .children(scope)
            .iter()
            .filter(|&&c| model.exists(c))
            .map(|&c| (c, model.size(c, mode)))
            .collect();
        let tiles = squarify(&items, area, aspect);
        let fills = Self::colour(model, theme, colors, scope, &items, &tiles);
        let hidden = items
            .iter()
            .filter(|&&(id, size)| size > 0 && !tiles.iter().any(|t| t.id == Some(id)))
            .copied()
            .collect();
        self.cache = Some(Cache {
            key,
            fingerprint,
            at: now,
            tiles,
            fills,
            hidden,
            child_count: items.len(),
            total: items.iter().map(|&(_, s)| s).sum(),
        });
        self.relayouts += 1;
    }

    /// Colours the tiles: files by type, folders by identity, avoiding the hues of the file types
    /// present in the scope folder.
    fn colour(
        model: &Model,
        theme: &Theme,
        colors: &mut TileColors,
        scope: NodeId,
        items: &[(NodeId, u64)],
        tiles: &[Tile],
    ) -> Vec<Fill> {
        let file_hue = |id: NodeId| theme.category_hue(model.category_of_node(id));
        let direct_files = items
            .iter()
            .filter(|&&(id, _)| model.kind(id) != Kind::Dir)
            .map(|&(id, _)| file_hue(id));
        let reserved = colors.reserve(scope, direct_files);
        let kinds: Vec<TileKind> = tiles
            .iter()
            .map(|t| match t.id {
                None => TileKind::Aggregate,
                Some(id) if model.kind(id) == Kind::Dir => TileKind::Folder(id),
                Some(id) => TileKind::File(file_hue(id)),
            })
            .collect();
        let rects: Vec<Rect> = tiles.iter().map(|t| t.rect).collect();
        let (fills, memory) = assign(&kinds, &adjacency(&rects), reserved, &colors.folder_colors);
        colors.folder_colors = memory;
        fills
    }

    /// The tile reached by moving from the highlighted one (or the largest, if none is).
    pub fn step(&self, tree: &TreeState, dir: Direction) -> Option<Tile> {
        let tiles = self.tiles();
        match self.selected_index(tree) {
            Some(from) => neighbor(tiles, from, dir).map(|i| tiles[i]),
            None => tiles.first().copied(),
        }
    }
}
