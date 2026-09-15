use crate::bounds::Bounds;
use crate::coords::Coord;

#[derive(Debug, Clone, PartialEq)]
pub struct Layer<T> {
    bounds: Bounds,
    cells: Box<[T]>,
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
}

#[cfg(test)]
mod tests;
