use std::collections::HashMap;
use serde_json::Value;
use crate::coords::GridSize;
use crate::tile::TileId;
use crate::tile::EMPTY;

pub struct Layer {
    pub name: String,
    cells: Vec<TileId>,
    metadata: HashMap<usize, HashMap<String, Value>>,
    size: GridSize,
}

impl Layer{

    pub fn new(name: String, size: GridSize) -> Self {
        let volume = size.volume();
        Layer { 
            name, 
            cells: vec![EMPTY; volume], 
            metadata: HashMap::new(),
            size 
        }
    }

    fn index(&self, x: i32, y: i32, z: i32) -> usize {
        (z as usize * self.size.width as usize * self.size.height as usize) 
        + (y as usize * self.size.width as usize) 
        + x as usize
    }

    fn in_bounds(&self, x: i32, y: i32, z: i32) -> bool {
        x >= 0 && (x as u32) < self.size.width &&
        y >= 0 && (y as u32) < self.size.height &&
        z >= 0 && (z as u32) < self.size.depth
    }

    pub fn get(&self, x: i32, y: i32, z: i32) -> Option<TileId> {
        if self.in_bounds(x, y, z) {
            let index = self.index(x, y, z);
            Some(self.cells[index])
        } else {
            None
        }
    }

    pub fn get_or_empty(&self, x: i32, y: i32, z: i32) -> TileId {
        self.get(x, y, z).unwrap_or(EMPTY)
    }

    pub fn metadata(&self, x: i32, y: i32, z: i32) -> Option<&HashMap<String, Value>> {
        if self.in_bounds(x, y, z) {
            self.metadata.get(&self.index(x, y, z))
        } else {
            None
        }
    }

    pub fn set_metadata(&mut self, x: i32, y: i32, z: i32, key: &str, value: Value) -> bool {
        if self.in_bounds(x, y, z) {
            let index = self.index(x, y, z);
            self.metadata.entry(index).or_default().insert(key.to_string(), value);
            true
        } else {
            false
        }
    }

    /// Flat cell storage in z-major/y-major order — feeds RLE export.
    pub fn cells(&self) -> &[TileId] {
        &self.cells
    }

    pub fn set(&mut self, x: i32, y: i32, z: i32, tile_id: TileId) -> bool {
        if self.in_bounds(x, y, z) {
            let index = self.index(x, y, z);
            self.cells[index] = tile_id;
            true
        } else {
            false
        }
    }
}