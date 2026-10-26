//! A special popup (ZC_SPECIALPOPUP): the server names one by id. The picture and the lines live in
//! System/spopup.lub.
//!
//! ponytail: that table isn't read yet, so the box shows the id. A click or Esc closes it.

use crate::window::Chrome;
use crate::{Layer, contains, pickup, screen};

pub const SIZE: [f32; 2] = [280.0, 90.0];

/// Centred horizontally, a third of the way down.
pub fn place() -> [f32; 2] {
    let screen = screen();
    [((screen[0] - SIZE[0]) / 2.0).floor(), ((screen[1] - SIZE[1]) / 3.0).floor()]
}

pub fn covers(cursor: [f32; 2]) -> bool {
    contains(place(), SIZE, cursor)
}

pub fn draw<'a>(layer: &mut Layer<'a>, chrome: &'a Chrome, frame: &'a pickup::Assets, id: i32) {
    let at = place();
    pickup::draw_sysbox(layer, chrome, frame, [at[0], at[1], SIZE[0], SIZE[1]]);
    layer.texts.push((id.to_string(), [at[0] + 16.0, at[1] + 36.0], [0, 0, 0]));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sits_in_the_middle_and_covers_its_box() {
        let at = place();
        assert_eq!(at[0] + SIZE[0] / 2.0, screen()[0] / 2.0);
        assert!(covers([at[0] + 1.0, at[1] + 1.0]));
        assert!(!covers([at[0] - 1.0, at[1]]));
    }
}
