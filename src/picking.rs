//! What the pointer is on in the world: each unit's and ground item's clickable area, and which one wins.

/// A screen rectangle in logical pixels: left, top, right, bottom.
pub type Rect = [f32; 4];

/// The smallest clickable width and height of a unit: 40 px per 640 of screen width.
pub const UNIT_MIN: f32 = 64.0;
/// Of an item on the ground: 34 px per 640.
pub const ITEM_MIN: f32 = 54.0;

/// What a target is, for which wins where areas overlap.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Monster,
    Item,
    Npc,
    Homunculus,
    Player,
    /// Pets, mercenaries and the rest.
    Other,
}

impl Kind {
    /// Which wins where areas overlap, higher first: monsters, then items, NPCs and homunculi, then players. With
    /// `players_first` (Shift held, or aiming a skill for players) players come first and monsters and NPCs drop.
    fn rank(self, players_first: bool) -> u16 {
        match (self, players_first) {
            (Kind::Monster, false) | (Kind::Player, true) => 400,
            (Kind::Item, _) => 300,
            (Kind::Npc, false) | (Kind::Homunculus, _) | (Kind::Monster, true) => 200,
            (Kind::Player, false) | (Kind::Npc, true) => 100,
            (Kind::Other, _) => 50,
        }
    }
}

/// Something clickable in the world, as last drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Target<T> {
    pub owner: T,
    pub kind: Kind,
    /// The sprite's box as drawn: every layer's whole image.
    pub sprite: Rect,
    /// Where a click counts (see `area`).
    pub area: Rect,
    /// Squared distance from the camera: the nearest is drawn on top.
    pub depth: f32,
}

/// The clickable area of a sprite standing on a cell: its box grown evenly to at least `min` wide and tall, joined
/// with the cell's own box on screen, so a click on the cell under a unit or an item counts as on it.
pub fn area(sprite: Rect, cell: Rect, min: f32) -> Rect {
    let grow = |low: f32, high: f32| {
        let short = (min - (high - low)).max(0.0) / 2.0;
        [low - short, high + short]
    };
    let ([left, right], [top, bottom]) = (grow(sprite[0], sprite[2]), grow(sprite[1], sprite[3]));
    [left.min(cell[0]), top.min(cell[1]), right.max(cell[2]), bottom.max(cell[3])]
}

/// The screen box around points (a cell's corners).
pub fn bounds(points: &[[f32; 2]]) -> Rect {
    points.iter().fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |[left, top, right, bottom], &[x, y]| [left.min(x), top.min(y), right.max(x), bottom.max(y)])
}

/// How far a point is outside a rectangle (0 inside).
fn outside([x, y]: [f32; 2], [left, top, right, bottom]: Rect) -> f32 {
    f32::hypot((left - x).max(x - right).max(0.0), (top - y).max(y - bottom).max(0.0))
}

/// What the pointer picks among the targets whose area it's in: the highest kind (see `Kind::rank`), then the one
/// whose sprite it's on or nearest, then the nearest to the camera.
pub fn pick<T: Copy>(targets: impl IntoIterator<Item = Target<T>>, cursor: [f32; 2], players_first: bool) -> Option<T> {
    let [x, y] = cursor;
    targets
        .into_iter()
        .filter(|Target { area: [left, top, right, bottom], .. }| x >= *left && x < *right && y >= *top && y < *bottom)
        .min_by(|a, b| {
            (b.kind.rank(players_first).cmp(&a.kind.rank(players_first)))
                .then(outside(cursor, a.sprite).total_cmp(&outside(cursor, b.sprite)))
                .then(a.depth.total_cmp(&b.depth))
        })
        .map(|target| target.owner)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(owner: u32, kind: Kind, sprite: Rect, cell: Rect, depth: f32) -> Target<u32> {
        let min = if kind == Kind::Item { ITEM_MIN } else { UNIT_MIN };
        Target { owner, kind, sprite, area: area(sprite, cell, min), depth }
    }

    #[test]
    fn small_sprites_grow_to_the_minimum_around_their_middle() {
        // A Poring at the default zoom, 50 by 45 on a 41 by 31 cell under its feet.
        assert_eq!(area([100.0, 100.0, 150.0, 145.0], [105.0, 125.0, 146.0, 156.0], UNIT_MIN), [93.0, 90.5, 157.0, 156.0]);
        // Zoomed out: 19 by 18 on a 17 by 13 cell.
        assert_eq!(area([100.0, 100.0, 119.0, 118.0], [101.0, 111.0, 118.0, 124.0], UNIT_MIN), [77.5, 77.0, 141.5, 141.0]);
        // A potion: 26 by 28.
        assert_eq!(area([200.0, 200.0, 226.0, 228.0], [193.0, 207.0, 234.0, 238.0], ITEM_MIN), [186.0, 187.0, 240.0, 241.0]);
        // A tall NPC only grows sideways, and down to its cell's near corner.
        assert_eq!(area([0.0, 0.0, 40.0, 100.0], [0.0, 90.0, 40.0, 110.0], UNIT_MIN), [-12.0, 0.0, 52.0, 110.0]);
    }

    #[test]
    fn a_click_off_a_small_sprite_counts() {
        let mushroom = target(1, Kind::Monster, [547.0, 364.0, 572.0, 396.0], [539.0, 371.0, 580.0, 401.0], 1.0);
        assert_eq!(pick([mushroom], [560.0, 380.0], false), Some(1));
        // 8 px under its sprite, and 10 px to its side.
        assert_eq!(pick([mushroom], [560.0, 404.0], false), Some(1));
        assert_eq!(pick([mushroom], [582.0, 380.0], false), Some(1));
        // Well away.
        assert_eq!(pick([mushroom], [560.0, 420.0], false), None);
        assert_eq!(pick([mushroom], [600.0, 380.0], false), None);
    }

    #[test]
    fn monsters_win_over_items_npcs_and_players_unless_players_come_first() {
        let monster = target(1, Kind::Monster, [100.0, 100.0, 150.0, 145.0], [105.0, 125.0, 146.0, 156.0], 9.0);
        let item = target(2, Kind::Item, [110.0, 130.0, 136.0, 158.0], [105.0, 125.0, 146.0, 156.0], 8.0);
        let npc = target(3, Kind::Npc, [90.0, 40.0, 140.0, 150.0], [95.0, 130.0, 136.0, 160.0], 7.0);
        let player = target(4, Kind::Player, [95.0, 30.0, 145.0, 151.0], [100.0, 130.0, 141.0, 161.0], 1.0);
        let all = [monster, item, npc, player];
        // Everyone is under the pointer: the monster, though the player is nearer the camera and the item is on top of it.
        assert_eq!(pick(all, [120.0, 140.0], false), Some(1));
        assert_eq!(pick([item, npc, player], [120.0, 140.0], false), Some(2));
        assert_eq!(pick([npc, player], [120.0, 140.0], false), Some(3));
        // Shift held: the player.
        assert_eq!(pick(all, [120.0, 140.0], true), Some(4));
    }

    #[test]
    fn among_the_same_kind_the_sprite_under_the_pointer_wins_then_the_nearest() {
        // Two Porings, the second a little behind and to the right: their grown areas overlap.
        let front = target(1, Kind::Monster, [100.0, 100.0, 150.0, 145.0], [105.0, 125.0, 146.0, 156.0], 4.0);
        let back = target(2, Kind::Monster, [140.0, 90.0, 190.0, 135.0], [145.0, 115.0, 186.0, 146.0], 5.0);
        // On the back one's sprite, in the front one's area too: the back one.
        assert_eq!(pick([front, back], [153.0, 120.0], false), Some(2));
        // Where the sprites overlap: the front one, drawn on top.
        assert_eq!(pick([front, back], [145.0, 120.0], false), Some(1));
        // Off both sprites, in both areas: the nearer sprite.
        assert_eq!(pick([front, back], [152.0, 140.0], false), Some(1));
        assert_eq!(pick([front, back], [138.0, 95.0], false), Some(2));
    }
}
