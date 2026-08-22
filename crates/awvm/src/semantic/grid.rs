//! Board-shaped side tables.
//!
//! Almost every question this crate answers about a position is answered by
//! walking something once and remembering the result per tile: where each unit
//! stands, what a tile costs to enter, which tiles a search has settled. Each
//! of those is a map that shadows the board, so each one used to restate the
//! same row-major arithmetic and the same bounds check.
//!
//! [`Dimensions`] is that arithmetic, held once. [`Grid`] is a map built on it,
//! keyed by [`Pos`] instead of by an index the caller computes.

use std::ops::{Index, IndexMut};

use super::Pos;

/// How large a board is, and therefore how large every map over it is.
///
/// Two grids built from the same `Dimensions` agree on what each index means.
/// A grid built from a different board does not, which is why the accessors
/// bounds-check rather than trusting a caller's index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Dimensions {
    width: u8,
    height: u8,
}

impl Dimensions {
    /// Maximum width or height supported by the VM.
    pub const MAX_AXIS: u8 = u8::MAX;

    pub const fn new(width: u8, height: u8) -> Self {
        Self { width, height }
    }

    #[inline]
    pub const fn width(self) -> u8 {
        self.width
    }

    #[inline]
    pub const fn height(self) -> u8 {
        self.height
    }

    /// How many tiles a map over this board holds. A board is never empty:
    /// `Board::new` rejects a zero width or height.
    #[inline]
    pub const fn len(self) -> usize {
        self.width as usize * self.height as usize
    }

    /// Whether a coordinate is on the board.
    #[inline]
    pub const fn contains(self, position: Pos) -> bool {
        position.x < self.width && position.y < self.height
    }

    /// Whether this board holds no tiles at all, which a decoded board never
    /// does.
    pub const fn is_empty(self) -> bool {
        self.len() == 0
    }

    /// Where `position` lives in a row-major map, or `None` off the board.
    #[inline]
    pub const fn index(self, position: Pos) -> Option<usize> {
        if self.contains(position) {
            Some(position.y as usize * self.width as usize + position.x as usize)
        } else {
            None
        }
    }

    /// `position` as a [`Cell`] of this shape, or `None` off the board.
    ///
    /// Ask once and read every table beside it with the answer.
    #[inline]
    pub const fn cell(self, position: Pos) -> Option<Cell> {
        match self.index(position) {
            Some(index) => Some(Cell { index, position }),
            None => None,
        }
    }

    /// `position` as a [`CellIdx`] of this shape, or `None` off the board.
    ///
    /// This is [`Dimensions::cell`] in two bytes. A [`Cell`] carries its
    /// coordinate so that a table read costs no arithmetic. A `CellIdx` drops
    /// the coordinate so that a value that must stay small, such as an order a
    /// search keeps by the million, can name a tile at all.
    #[inline]
    pub const fn cell_index(self, position: Pos) -> Option<CellIdx> {
        match self.index(position) {
            // A board is at most 255x255, so an index over it is at most
            // 65,025 and always fits.
            Some(index) => Some(CellIdx(index as u16)),
            None => None,
        }
    }

    /// The coordinate `index` names on this board, or `None` past its end.
    #[inline]
    pub const fn position_of(self, index: CellIdx) -> Option<Pos> {
        let index = index.0 as usize;
        if index >= self.len() {
            return None;
        }
        let width = self.width as usize;
        Some(Pos {
            x: (index % width) as u8,
            y: (index / width) as u8,
        })
    }

    /// Every coordinate, row by row.
    pub fn positions(self) -> impl Iterator<Item = Pos> {
        (0..self.height).flat_map(move |y| (0..self.width).map(move |x| Pos { x, y }))
    }
}

/// A coordinate already checked against one board shape, with where it lives in
/// a row-major map over that shape.
///
/// One turn asks several board-shaped tables about the same tile: what terrain
/// it holds, who stands on it, what it costs to enter, whether a route may
/// cross it, and how a search arrived there. Every answer recomputed
/// `y * width + x` and bounds-checked it again. A cell is that work done once.
///
/// Only a [`Dimensions`] mints one, and every table a turn holds is built from
/// one `Dimensions`, so a cell means the same tile in all of them. Read against
/// a grid of a different shape it panics, exactly as indexing a slice past its
/// end does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    index: usize,
    position: Pos,
}

impl Cell {
    /// The coordinate this cell names.
    pub const fn position(self) -> Pos {
        self.position
    }
}

/// Where a tile lives in a row-major map, in two bytes.
///
/// A board is at most 255 by 255, because [`Pos`] holds two `u8` fields, so
/// every index over one fits a `u16`. That lets an order name its destination
/// without a coordinate pair and without a lifetime.
///
/// An index alone says nothing about which board it came from. Mint one with
/// [`Dimensions::cell_index`] and read it back with
/// [`Dimensions::position_of`]. Both check it against a shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CellIdx(u16);

impl CellIdx {
    /// The raw row-major index.
    pub const fn get(self) -> u16 {
        self.0
    }

    /// An index built from a raw number.
    ///
    /// Nothing here says the number names a tile of any particular board.
    /// [`Dimensions::position_of`] decides that.
    pub const fn from_raw(index: u16) -> Self {
        Self(index)
    }
}

/// One value per tile, row-major.
///
/// Indexing by [`Pos`] panics off the board, in the same way indexing a slice
/// panics past its end: a coordinate that came from this grid's own dimensions
/// is always in range, and one that did not is a bug in the caller. Use
/// [`Grid::get`] where the coordinate is untrusted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grid<T> {
    dimensions: Dimensions,
    cells: Vec<T>,
}

impl<T> Grid<T> {
    /// A grid holding `value` everywhere.
    pub fn filled(dimensions: Dimensions, value: T) -> Self
    where
        T: Clone,
    {
        Self {
            dimensions,
            cells: vec![value; dimensions.len()],
        }
    }

    /// Fill this grid with `value` over `dimensions`, keeping its allocation.
    ///
    /// This is [`Grid::filled`] for a grid that is being reused. A search that
    /// runs once per unit of a turn wants it. The arrival grid is the only
    /// board-sized thing the search owns, and refilling one costs the write
    /// that building one costs anyway, without the allocation and the free.
    pub fn refill(&mut self, dimensions: Dimensions, value: T)
    where
        T: Clone,
    {
        self.cells.clear();
        self.cells.resize(dimensions.len(), value);
        self.dimensions = dimensions;
    }

    /// A grid over `dimensions` holding `cells`, row-major.
    ///
    /// `None` unless `cells` is exactly the rectangle `dimensions` describes,
    /// which is what makes every accessor below total.
    pub fn from_cells(dimensions: Dimensions, cells: Vec<T>) -> Option<Self> {
        (cells.len() == dimensions.len()).then_some(Self { dimensions, cells })
    }

    /// A grid whose value at each tile is what `cell` says, row by row.
    pub fn from_fn(dimensions: Dimensions, mut cell: impl FnMut(Pos) -> T) -> Self {
        let mut cells = Vec::with_capacity(dimensions.len());
        cells.extend(dimensions.positions().map(&mut cell));
        Self { dimensions, cells }
    }

    /// The shape this grid and every map beside it share.
    #[inline]
    pub const fn dimensions(&self) -> Dimensions {
        self.dimensions
    }

    #[inline]
    pub const fn width(&self) -> u8 {
        self.dimensions.width()
    }

    #[inline]
    pub const fn height(&self) -> u8 {
        self.dimensions.height()
    }

    #[inline]
    pub fn get(&self, position: Pos) -> Option<&T> {
        self.dimensions
            .index(position)
            .map(|index| &self.cells[index])
    }

    /// The value at `cell`, without checking a coordinate again.
    ///
    /// Panics when `cell` was minted by a larger board, in the same way
    /// indexing a slice past its end does. A cell from a board of a different
    /// shape names a different tile even when the index is in range, which a
    /// debug build reports rather than answering about the wrong tile.
    #[inline]
    pub fn at(&self, cell: Cell) -> &T {
        debug_assert!(self.holds(cell), "a cell from another board shape");
        &self.cells[cell.index]
    }

    /// [`Grid::at`], mutably.
    #[inline]
    pub fn at_mut(&mut self, cell: Cell) -> &mut T {
        debug_assert!(self.holds(cell), "a cell from another board shape");
        &mut self.cells[cell.index]
    }

    /// Whether `cell` names the tile this grid's own shape puts at that index.
    fn holds(&self, cell: Cell) -> bool {
        self.dimensions.index(cell.position()) == Some(cell.index)
    }

    #[inline]
    pub fn get_mut(&mut self, position: Pos) -> Option<&mut T> {
        self.dimensions
            .index(position)
            .map(|index| &mut self.cells[index])
    }

    /// Every tile with its coordinate, row by row.
    ///
    /// Walking rows rather than indices is what keeps a coordinate free: a
    /// row-major index only becomes a coordinate through a division, and a
    /// caller sweeping the board pays that on every tile it looks at.
    pub fn iter(&self) -> impl Iterator<Item = (Pos, &T)> {
        let width = usize::from(self.dimensions.width()).max(1);
        self.cells.chunks(width).enumerate().flat_map(|(y, row)| {
            row.iter()
                .enumerate()
                .map(move |(x, cell)| (Pos::new(x as u8, y as u8), cell))
        })
    }

    /// Every coordinate this grid covers, row by row.
    pub fn positions(&self) -> impl Iterator<Item = Pos> + use<T> {
        self.dimensions.positions()
    }

    /// Every value, row by row, without its coordinate.
    pub fn cells(&self) -> impl Iterator<Item = &T> {
        self.cells.iter()
    }

    pub fn cells_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.cells.iter_mut()
    }

    /// The grid as rows, for the wire shapes that are nested.
    pub fn rows(&self) -> impl Iterator<Item = impl Iterator<Item = (Pos, &T)>> {
        let width = usize::from(self.dimensions.width()).max(1);
        self.cells.chunks(width).enumerate().map(|(y, row)| {
            row.iter()
                .enumerate()
                .map(move |(x, cell)| (Pos::new(x as u8, y as u8), cell))
        })
    }
}

/// A grid over a board with no tiles, which every coordinate is off.
///
/// This is what a table looks like before the board it shadows is known.
impl<T> Default for Grid<T> {
    fn default() -> Self {
        Self {
            dimensions: Dimensions::new(0, 0),
            cells: Vec::new(),
        }
    }
}

impl<T> Index<Pos> for Grid<T> {
    type Output = T;

    fn index(&self, position: Pos) -> &T {
        self.get(position).unwrap_or_else(|| {
            panic!(
                "{position} is off a {}x{} grid",
                self.width(),
                self.height()
            )
        })
    }
}

impl<T> IndexMut<Pos> for Grid<T> {
    fn index_mut(&mut self, position: Pos) -> &mut T {
        let (width, height) = (self.width(), self.height());
        self.get_mut(position)
            .unwrap_or_else(|| panic!("{position} is off a {width}x{height} grid"))
    }
}
