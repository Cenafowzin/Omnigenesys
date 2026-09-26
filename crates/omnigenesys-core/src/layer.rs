use crate::bounds::Bounds;
use crate::coords::{Coord, GridSize};
use crate::tile::TileId;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq)]
pub struct Layer<T> {
    bounds: Bounds,
    cells: Box<[T]>,
}

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub enum AnyLayer {
    Tiles(Layer<TileId>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LayerKind {
    Tiles,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("coord {coord:?} is outside bounds {bounds:?}")]
pub struct OutOfBounds {
    pub coord: Coord,
    pub bounds: Bounds,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("grid size {size:?} has too many cells to index")]
pub struct TooLarge {
    pub size: GridSize,
}

impl<T: Clone> Layer<T> {
    pub fn new(bounds: Bounds, fill: T) -> Result<Layer<T>, TooLarge> {
        let cell_count = bounds
            .size
            .checked_len()
            .ok_or(TooLarge { size: bounds.size })?;
        let cells = vec![fill; cell_count].into_boxed_slice();
        Ok(Layer { bounds, cells })
    }
}

impl<T> Layer<T> {
    #[inline]
    fn index(&self, c: Coord) -> Option<usize> {
        if self.bounds.contains(c) {
            let local = c - self.bounds.origin;
            let index = local.x as usize
                + self.bounds.size.width as usize
                    * (local.y as usize + self.bounds.size.height as usize * local.z as usize);
            Some(index)
        } else {
            None
        }
    }

    pub fn bounds(&self) -> Bounds {
        self.bounds
    }

    #[inline]
    pub fn get(&self, c: Coord) -> Option<&T> {
        self.index(c).map(|i| &self.cells[i])
    }

    #[inline]
    pub fn set(&mut self, c: Coord, value: T) -> Result<(), OutOfBounds> {
        let i = self.index(c).ok_or(OutOfBounds {
            coord: c,
            bounds: self.bounds,
        })?;
        self.cells[i] = value;
        Ok(())
    }

    pub fn cells(&self) -> impl Iterator<Item = &T> {
        self.cells.iter()
    }

    pub fn cells_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.cells.iter_mut()
    }

    pub fn iter(&self) -> impl Iterator<Item = (Coord, &T)> {
        let Bounds { origin, size } = self.bounds;
        self.cells
            .chunks_exact((size.height as usize * size.width as usize).max(1))
            .enumerate()
            .flat_map(move |(z, plane)| {
                plane
                    .chunks_exact((size.width as usize).max(1))
                    .enumerate()
                    .flat_map(move |(y, row)| {
                        row.iter().enumerate().map(move |(x, cell)| {
                            let coord = Coord::new(
                                origin.x + x as i32,
                                origin.y + y as i32,
                                origin.z + z as i32,
                            );
                            (coord, cell)
                        })
                    })
            })
    }
}

impl AnyLayer {
    #[inline]
    pub fn kind(&self) -> LayerKind {
        match self {
            AnyLayer::Tiles(_) => LayerKind::Tiles,
        }
    }

    #[inline]
    pub fn bounds(&self) -> Bounds {
        match self {
            AnyLayer::Tiles(layer) => layer.bounds(),
        }
    }

    #[inline]
    pub fn as_tiles(&self) -> Option<&Layer<TileId>> {
        match self {
            AnyLayer::Tiles(layer) => Some(layer),
        }
    }

    #[inline]
    pub fn as_tiles_mut(&mut self) -> Option<&mut Layer<TileId>> {
        match self {
            AnyLayer::Tiles(layer) => Some(layer),
        }
    }
}

impl std::fmt::Display for LayerKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LayerKind::Tiles => write!(f, "tiles"),
        }
    }
}

#[cfg(test)]
mod tests;
