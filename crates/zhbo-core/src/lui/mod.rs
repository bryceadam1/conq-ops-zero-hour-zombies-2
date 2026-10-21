//! BO2's LUI layout model, shared by the HUD and the create-a-class screens so a port from BO2's
//! UI Lua (`~/Documents/BO2-reference/lua-dump/decompiled/patch_mp/ui/lui/`) keeps its numbers
//! and its anchoring.
//!
//! Every LUI element is a box given by four edge offsets and four anchor flags
//! (`LUI.UIElement.new{left, top, right, bottom, leftAnchor, topAnchor, rightAnchor,
//! bottomAnchor}`, e.g. `hud/ammocounter.lua`). Each axis resolves against the parent the same way:
//! both anchors set, each offset is from its own parent edge (the box stretches); one anchor set,
//! both offsets are from that edge; neither set, both are from the parent's centre. The root is
//! [`LUI_HEIGHT`] virtual units tall (`lui.lua`: `LUI.UIHeight = 720`) and as wide as the screen's
//! aspect makes it, so a 16:9 screen is 1280 x 720 units; [`Root::to_screen`] scales to pixels.
//! The engine centres the root on the screen and PC's safe area is the whole root (Ghidra G9),
//! which this top-left-origin root matches.

pub mod anim;
pub mod font;
pub mod materials;

/// `LUI.UIHeight`.
pub const LUI_HEIGHT: f32 = 720.0;

/// A resolved box, in the units of whatever it was resolved in (LUI units or screen pixels).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
}

/// An element's placement: `LUI.UIElement.new`'s eight fields, or `setLeftRight`/`setTopBottom`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    pub left_anchor: bool,
    pub right_anchor: bool,
    pub left: f32,
    pub right: f32,
    pub top_anchor: bool,
    pub bottom_anchor: bool,
    pub top: f32,
    pub bottom: f32,
}

impl Place {
    /// `setLeftRight(left_anchor, right_anchor, left, right)` + `setTopBottom(top_anchor,
    /// bottom_anchor, top, bottom)`, in LUI's argument order.
    pub const fn new(lr: (bool, bool, f32, f32), tb: (bool, bool, f32, f32)) -> Place {
        Place {
            left_anchor: lr.0,
            right_anchor: lr.1,
            left: lr.2,
            right: lr.3,
            top_anchor: tb.0,
            bottom_anchor: tb.1,
            top: tb.2,
            bottom: tb.3,
        }
    }

    /// The whole parent (`setLeftRight(true, true, 0, 0)`, `setTopBottom(true, true, 0, 0)`).
    pub const FILL: Place = Place::new((true, true, 0.0, 0.0), (true, true, 0.0, 0.0));

    /// This box inside `parent`.
    pub fn resolve(&self, parent: Rect) -> Rect {
        let (x0, x1) = axis(parent.x, parent.right(), self.left_anchor, self.right_anchor, self.left, self.right);
        let (y0, y1) = axis(parent.y, parent.bottom(), self.top_anchor, self.bottom_anchor, self.top, self.bottom);
        Rect { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }
    }
}

fn axis(lo: f32, hi: f32, lo_anchor: bool, hi_anchor: bool, a: f32, b: f32) -> (f32, f32) {
    match (lo_anchor, hi_anchor) {
        (true, true) => (lo + a, hi + b),
        (true, false) => (lo + a, lo + b),
        (false, true) => (hi + a, hi + b),
        (false, false) => {
            let mid = (lo + hi) * 0.5;
            (mid + a, mid + b)
        }
    }
}

/// The LUI root for one screen: [`LUI_HEIGHT`] units tall, the screen's aspect wide.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Root {
    /// Screen pixels per LUI unit.
    pub scale: f32,
    /// The root's box in LUI units (origin top-left).
    pub rect: Rect,
}

impl Root {
    pub fn new(screen_w: f32, screen_h: f32) -> Root {
        let scale = (screen_h / LUI_HEIGHT).max(f32::MIN_POSITIVE);
        Root { scale, rect: Rect { x: 0.0, y: 0.0, w: screen_w / scale, h: LUI_HEIGHT } }
    }

    /// A box in LUI units, in screen pixels.
    pub fn to_screen(&self, r: Rect) -> Rect {
        Rect { x: r.x * self.scale, y: r.y * self.scale, w: r.w * self.scale, h: r.h * self.scale }
    }

    /// An element placed directly in the root, in screen pixels.
    pub fn place(&self, p: &Place) -> Rect {
        self.to_screen(p.resolve(self.rect))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PARENT: Rect = Rect { x: 100.0, y: 50.0, w: 200.0, h: 40.0 };

    #[test]
    fn each_anchor_combination_resolves_like_lui() {
        // Both anchors: offsets from their own edges (the box stretches with the parent).
        let both = Place::new((true, true, 10.0, -10.0), (true, true, 0.0, 0.0)).resolve(PARENT);
        assert_eq!(both, Rect { x: 110.0, y: 50.0, w: 180.0, h: 40.0 });
        // Left only: both offsets from the left edge (`hud/ammocounter.lua`'s -90..10 box).
        let left = Place::new((true, false, -90.0, 10.0), (true, false, 36.0, 76.0)).resolve(PARENT);
        assert_eq!(left, Rect { x: 10.0, y: 86.0, w: 100.0, h: 40.0 });
        // Right only: both from the right edge.
        let right = Place::new((false, true, -50.0, 0.0), (false, true, -20.0, 0.0)).resolve(PARENT);
        assert_eq!(right, Rect { x: 250.0, y: 70.0, w: 50.0, h: 20.0 });
        // Neither: both from the centre.
        let mid = Place::new((false, false, -25.0, 25.0), (false, false, -5.0, 5.0)).resolve(PARENT);
        assert_eq!(mid, Rect { x: 175.0, y: 65.0, w: 50.0, h: 10.0 });
    }

    #[test]
    fn the_root_is_720_units_tall_and_as_wide_as_the_aspect() {
        let r = Root::new(1920.0, 1080.0);
        assert_eq!(r.rect.w, 1280.0);
        assert_eq!(r.scale, 1.5);
        let wide = Root::new(2560.0, 1080.0);
        assert!((wide.rect.w - 1706.6666).abs() < 0.01);
        let px = r.place(&Place::new((false, true, -100.0, 0.0), (false, true, -50.0, 0.0)));
        assert_eq!(px, Rect { x: 1770.0, y: 1005.0, w: 150.0, h: 75.0 });
    }
}
