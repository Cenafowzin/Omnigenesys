use crate::bounds::Bounds;
use crate::coords::Coord;
use crate::tile::TileId;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutOfBounds {
    pub coord: Coord,
    pub bounds: Bounds,
}

impl<T: Clone> Layer<T> {
    pub fn new(bounds: Bounds, fill: T) -> Layer<T> {
        let cells = vec![fill; bounds.size.len()].into_boxed_slice();
        Layer { bounds, cells }
    }
}

impl<T> Layer<T> {
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

    pub fn get(&self, c: Coord) -> Option<&T> {
        self.index(c).map(|i| &self.cells[i])
    }

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
        self.bounds.iter().zip(self.cells.iter())
    }
}

impl AnyLayer {
    pub fn kind(&self) -> LayerKind {
        match self {
            AnyLayer::Tiles(_) => LayerKind::Tiles,
        }
    }

    pub fn bounds(&self) -> Bounds {
        match self {
            AnyLayer::Tiles(layer) => layer.bounds(),
        }
    }

    pub fn as_tiles(&self) -> Option<&Layer<TileId>> {
        match self {
            AnyLayer::Tiles(layer) => Some(layer),
        }
    }

    pub fn as_tiles_mut(&mut self) -> Option<&mut Layer<TileId>> {
        match self {
            AnyLayer::Tiles(layer) => Some(layer),
        }
    }
}

#[cfg(test)]
mod tests;
