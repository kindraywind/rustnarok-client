//! The cart choice ("Cart List"): Change Cart opens it with the carts the base level allows, Change Cart 2 with the
//! server's list (ZC_SELECTCART). Each cart shows as its world sprite, five to a row; a click on one sends its type and
//! closes it, the close button closes it and sends nothing. Esc leaves it up, as in the game.

use std::collections::HashMap;

use crate::actor::{self, SpriteSet, Timing};
use crate::render::{Gpu, Texture};
use crate::window::{BUTTON_SIZE, Chrome, Frame, TITLE_HEIGHT};
use crate::{Layer, UI, contains, offset};
use rustnarok_client::grf::Grf;

/// Change Cart's SP, which the client checks before opening the choice.
pub const SP: i64 = 40;
/// "Insufficient SP", "You must have a cart.": the red lines when it doesn't open.
pub const REFUSALS: [usize; 2] = [202, 660];
/// "Cart List".
const TITLE: usize = 659;
const WIDTH: f32 = 385.0;
/// The window grows this much a row, though the carts step 82 down.
const ROW_HEIGHT: f32 = 98.0;
const PER_ROW: usize = 5;
/// A cart's box: 100 × 100, one every 70 across and 82 down from (0, 28).
const CHOICE: [f32; 2] = [100.0, 100.0];
const STEP: [f32; 2] = [70.0, 82.0];
const FIRST_TOP: f32 = 28.0;

/// Change Cart's carts and the base level each needs above (rathena's clif_parse_ChangeCart, NEW_CARTS): the list
/// stops at the first one the level doesn't pass; the Mado cart (13) comes after them above 100.
const CHANGE_CART_TYPES: [(u16, i64); 9] = [(1, 0), (2, 40), (3, 65), (4, 80), (5, 90), (6, 100), (7, 110), (8, 120), (9, 130)];
const MADO_CART: (u16, i64) = (13, 100);

/// Where a pick goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Answer {
    /// CZ_REQ_CHANGECART.
    ChangeCart,
    /// CZ_SELECTCART, with the account ID the list came with.
    SelectCart(u32),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    Title,
    Close,
    Cart(u16),
    Window,
}

pub struct Assets {
    corners: [Texture; 2],
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        Self { corners: ["sysboxs_ld", "sysboxs_rd"].map(|name| gpu.load_texture(grf, &format!("{UI}basic_interface\\{name}.bmp"), false)) }
    }
}

/// Each cart's sprite (None: not in the data), and the pieces of the frame it shows.
pub type Sprites<'a, 'b> = (&'a HashMap<u16, Option<SpriteSet>>, &'b HashMap<u16, Vec<actor::Placed>>);

pub struct CartChoice {
    pub kinds: Vec<u16>,
    pub answer: Answer,
    pub frame: Frame,
}

/// Where it opens on a screen this size: 200 right of a 640-wide middle, 200 / 480 of the way down.
pub fn place(screen: [f32; 2]) -> [f32; 2] {
    [((screen[0] - 640.0) / 2.0 + 200.0).floor(), (200.0 * screen[1] / 480.0).floor()]
}

/// A cart's box nudged by the type, as the game places each sprite.
fn nudge(kind: u16) -> [f32; 2] {
    match kind {
        3 => [0.0, 0.0],
        4 => [18.0, -10.0],
        5 => [15.0, -5.0],
        6..=9 => [13.0, 0.0],
        11 => [16.0, -13.0],
        12 => [0.0, -23.0],
        13 => [4.0, -10.0],
        _ => [0.0, -13.0],
    }
}

impl CartChoice {
    /// Change Cart: the carts a character of this base level may take.
    pub fn change_cart(base_level: i64, screen: [f32; 2]) -> Self {
        let mut kinds: Vec<u16> = CHANGE_CART_TYPES.iter().take_while(|&&(_, above)| base_level > above).map(|&(kind, _)| kind).collect();
        if base_level > MADO_CART.1 {
            kinds.push(MADO_CART.0);
        }
        Self::new(kinds, Answer::ChangeCart, screen)
    }

    /// Change Cart 2: the server's list.
    pub fn select_cart(account: u32, kinds: &[u8], screen: [f32; 2]) -> Self {
        Self::new(kinds.iter().map(|&kind| u16::from(kind)).collect(), Answer::SelectCart(account), screen)
    }

    fn new(kinds: Vec<u16>, answer: Answer, screen: [f32; 2]) -> Self {
        let mut frame = Frame::new(place(screen), WIDTH);
        frame.open = true;
        Self { kinds, answer, frame }
    }

    pub fn size(&self) -> [f32; 2] {
        let rows = self.kinds.len().max(1).div_ceil(PER_ROW);
        [WIDTH, rows as f32 * ROW_HEIGHT]
    }

    /// Each cart and its box's corner inside the window.
    fn boxes(&self) -> impl Iterator<Item = (u16, [f32; 2])> + '_ {
        self.kinds.iter().enumerate().map(|(index, &kind)| {
            let [col, row] = [(index % PER_ROW) as f32, (index / PER_ROW) as f32];
            (kind, offset([col * STEP[0], FIRST_TOP + row * STEP[1]], nudge(kind)))
        })
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Hit> {
        let local = self.frame.local(cursor, self.size()[1])?;
        if contains(self.frame.close_button(), BUTTON_SIZE, local) {
            return Some(Hit::Close);
        }
        if local[1] < TITLE_HEIGHT {
            return Some(Hit::Title);
        }
        // ponytail: the boxes overlap; the later one takes the press (the game's order among its buttons wasn't read).
        Some(self.boxes().filter(|&(_, corner)| contains(corner, CHOICE, local)).last().map_or(Hit::Window, |(kind, _)| Hit::Cart(kind)))
    }

    /// The close button and the carts on screen (for the test driver), as `cart:<type>`.
    pub fn button_boxes(&self) -> Vec<(String, [f32; 4])> {
        let place = |name: String, local: [f32; 2], [w, h]: [f32; 2]| {
            let [x, y] = self.frame.at(local);
            (name, [x, y, w, h])
        };
        let close = place("cart_choice_close".into(), self.frame.close_button(), BUTTON_SIZE);
        std::iter::once(close).chain(self.boxes().map(|(kind, corner)| place(format!("cart:{kind}"), corner, CHOICE))).collect()
    }

    /// The title bar (the base button and the close button), the white body with its rounded bottom corners, and each
    /// cart's first frame facing down, its first piece in the middle of its box.
    pub fn draw<'a>(
        &self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        assets: &'a Assets,
        (sprites, frames): Sprites<'a, '_>,
        messages: &[String],
        cursor: [f32; 2],
    ) {
        let size = self.size();
        let corner = self.frame.position;
        self.frame.draw_title_only(layer, chrome, messages.get(TITLE).map_or("", String::as_str));
        let base = chrome.button(0, false);
        layer.sprites.push((base, self.frame.at([3.0, 3.0]), base.size));
        let close = chrome.button(2, self.hit(cursor) == Some(Hit::Close));
        layer.sprites.push((close, self.frame.at(self.frame.close_button()), close.size));
        chrome.draw_body(layer, corner, size);
        let [left, right] = &assets.corners;
        layer.sprites.push((left, offset(corner, [0.0, size[1] - left.size[1]]), left.size));
        layer.sprites.push((right, offset(corner, [size[0] - right.size[0], size[1] - right.size[1]]), right.size));
        // ponytail: not clipped to the 100 × 100 box as in the game; every cart's frame fits in it.
        for (kind, local) in self.boxes() {
            let (Some(Some(sprite)), Some(placed)) = (sprites.get(&kind), frames.get(&kind)) else { continue };
            let Some(&(_, first_at, first_size, _)) = placed.first() else { continue };
            let middle = self.frame.at([local[0] + CHOICE[0] / 2.0, local[1] + CHOICE[1] / 2.0]);
            let shift = [middle[0] - first_at[0] - first_size[0] / 2.0, middle[1] - first_at[1] - first_size[1] / 2.0].map(f32::round);
            for &(key, at, size, _) in placed {
                if let Some(texture) = sprite.texture_of(key) {
                    layer.sprites.push((texture, offset(shift, at), size));
                }
            }
        }
    }
}

/// The cart's sprite as the world draws a grown-up's: 손수레 … 손수레8, then the named carts.
pub fn load_sprite(grf: &Grf, kind: u16) -> Option<SpriteSet> {
    let (sprite, act) = actor::cart_path(i32::from(kind), 0);
    SpriteSet::with_act(grf, &sprite, &act)
}

/// Each cart's first frame of its first action, the pieces placed from the sprite's origin.
pub fn frames(gpu: &Gpu, sprites: &mut HashMap<u16, Option<SpriteSet>>, kinds: &[u16]) -> HashMap<u16, Vec<actor::Placed>> {
    kinds
        .iter()
        .filter_map(|&kind| {
            let sprite = sprites.get_mut(&kind)?.as_mut()?;
            Some((kind, sprite.frame_at(gpu, 0, 0, Timing::Span(0.0))))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: [f32; 2] = [1024.0, 768.0];

    #[test]
    fn change_cart_offers_the_carts_the_level_allows() {
        let kinds = |level| CartChoice::change_cart(level, SCREEN).kinds;
        assert_eq!(kinds(1), [1]);
        assert_eq!(kinds(41), [1, 2]);
        assert_eq!(kinds(90), [1, 2, 3, 4]);
        assert_eq!(kinds(99), [1, 2, 3, 4, 5]);
        assert_eq!(kinds(101), [1, 2, 3, 4, 5, 6, 13]);
        assert_eq!(kinds(131), [1, 2, 3, 4, 5, 6, 7, 8, 9, 13]);
        let select = CartChoice::select_cart(7, &[10, 11, 12], SCREEN);
        assert_eq!((select.kinds, select.answer), (vec![10, 11, 12], Answer::SelectCart(7)));
    }

    #[test]
    fn lays_out_five_carts_a_row() {
        assert_eq!(place(SCREEN), [392.0, 320.0]);
        let one_row = CartChoice::change_cart(99, SCREEN);
        assert_eq!(one_row.size(), [385.0, 98.0]);
        let two_rows = CartChoice::change_cart(101, SCREEN);
        assert_eq!(two_rows.size(), [385.0, 196.0]);
        let boxes: Vec<_> = two_rows.boxes().collect();
        assert_eq!(boxes[0], (1, [0.0, 15.0]));
        assert_eq!(boxes[3], (4, [228.0, 18.0]));
        assert_eq!(boxes[5], (6, [13.0, 110.0]));
        assert_eq!(boxes[6], (13, [74.0, 100.0]));
        // A press: the close button, the title bar, a cart (the later of two overlapping boxes), the body.
        let at = |local: [f32; 2]| [392.0 + local[0], 320.0 + local[1]];
        assert_eq!(one_row.hit(at([375.0, 6.0])), Some(Hit::Close));
        assert_eq!(one_row.hit(at([100.0, 6.0])), Some(Hit::Title));
        assert_eq!(one_row.hit(at([30.0, 50.0])), Some(Hit::Cart(1)));
        assert_eq!(one_row.hit(at([80.0, 50.0])), Some(Hit::Cart(2)));
        assert_eq!(one_row.hit(at([200.0, 20.0])), Some(Hit::Window));
        assert_eq!(one_row.hit(at([30.0, 99.0])), None);
    }
}
