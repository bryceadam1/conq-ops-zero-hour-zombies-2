//! BO2's content-grid links (`contentgrid.lua`) on a flat option index.

/// BO2's `CoD.ContentGrid.AddButton` wires each new cell as it is added (`contentgrid.lua:91-122`,
/// through `SetNavigation` at `:137-144`):
///
/// * the cell left of a new one gets `right` to it and the new one `left` back (same row);
/// * the new cell's `right` goes to the *first* cell of its row, and that cell's `left` back to
///   the new one — so Left/Right **wrap inside the row**; they never page, and a one-cell row keeps
///   neither link (BO2's `SetNavigation` skips a target that is the button itself);
/// * `up`/`down` link the same column across rows, and the first/last row that holds the column
///   link back to each other, so vertical moves **wrap within the column** (a partially filled last
///   row simply has no cells in the columns it does not reach).
///
/// The helpers below are those links, computed on the option list's flat index. `i` is clamped so a
/// stale highlight (the option list shrank under it) cannot index past the list.
pub fn grid_left(i: usize, n: usize, cols: usize) -> usize {
    if n == 0 || cols == 0 {
        return 0;
    }
    let i = i.min(n - 1);
    let first = row_start(i, cols);
    if i > first {
        i - 1
    } else {
        first + row_len(first, n, cols) - 1
    }
}

/// The cell right of `i`: the next in the row, or the row's first (`contentgrid.lua:100-107`).
pub fn grid_right(i: usize, n: usize, cols: usize) -> usize {
    if n == 0 || cols == 0 {
        return 0;
    }
    let i = i.min(n - 1);
    let first = row_start(i, cols);
    if i + 1 < first + row_len(first, n, cols) {
        i + 1
    } else {
        first
    }
}

/// The cell above `i` in its column, wrapping to the last row that has the column
/// (`contentgrid.lua:108-115` plus the back-links the later rows add).
pub fn grid_up(i: usize, n: usize, cols: usize) -> usize {
    grid_vertical(i, n, cols, -1)
}

/// The cell below `i` in its column, wrapping to the first row
/// (`contentgrid.lua:116-122` plus the back-links).
pub fn grid_down(i: usize, n: usize, cols: usize) -> usize {
    grid_vertical(i, n, cols, 1)
}

/// The first index of `i`'s row.
pub(super) fn row_start(i: usize, cols: usize) -> usize {
    i - i % cols
}

/// How many cells of the row starting at `first` exist (the last row may be short).
pub(super) fn row_len(first: usize, n: usize, cols: usize) -> usize {
    (n - first).min(cols)
}

/// Walk the cells of `i`'s column by `step`, wrapping (`contentgrid.lua:108-122`): each row's
/// `up`/`down` is overwritten by the row that neighbours it, and the first row's `up` ends on the
/// last row holding the column (and vice versa).
pub(super) fn grid_vertical(i: usize, n: usize, cols: usize, step: i32) -> usize {
    if n == 0 || cols == 0 {
        return 0;
    }
    let i = i.min(n - 1);
    let col = i % cols;
    // The rows that hold this column: every full row, plus the last one only if it reaches it.
    let rows = (n - col).div_ceil(cols);
    let row = i / cols;
    let next = (row as i32 + step).rem_euclid(rows as i32) as usize;
    col + next * cols
}
