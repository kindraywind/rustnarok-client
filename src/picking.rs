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
    /// On a siege map, a player outside our guild: they rank with monsters, the nearer one winning.
    Foe,
    /// Pets, mercenaries and the rest.
    Other,
}

impl Kind {
    /// Which wins where areas overlap, higher first: monsters, then items, NPCs and homunculi, then players. With
    /// `players_first` (Shift held, or aiming a skill for players) players come first and monsters and NPCs drop.
    fn rank(self, players_first: bool) -> u16 {
        match (self, players_first) {
            (Kind::Monster, false) | (Kind::Player, true) | (Kind::Foe, _) => 400,
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
    /// How far from the camera along the way it looks: the nearest wins among its kind.
    pub depth: f32,
}

/// The clickable area of a sprite: its box grown evenly to at least `min` wide and tall, nothing more.
pub fn area(sprite: Rect, min: f32) -> Rect {
    let grow = |low: f32, high: f32| {
        let short = (min - (high - low)).max(0.0) / 2.0;
        [low - short, high + short]
    };
    let ([left, right], [top, bottom]) = (grow(sprite[0], sprite[2]), grow(sprite[1], sprite[3]));
    [left, top, right, bottom]
}

/// The screen box around points (a cell's corners).
pub fn bounds(points: &[[f32; 2]]) -> Rect {
    points.iter().fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |[left, top, right, bottom], &[x, y]| [left.min(x), top.min(y), right.max(x), bottom.max(y)])
}

/// Which quarter of the screen (split at `middle`) a point is in: 0 top-left, 1 top-right, 2 bottom-left, 3
/// bottom-right.
fn quarter([x, y]: [f32; 2], middle: [f32; 2]) -> usize {
    usize::from(x >= middle[0]) + 2 * usize::from(y >= middle[1])
}

/// What the pointer picks among the targets whose area it's strictly inside. Each area belongs to the screen quarter
/// (split at `middle`) holding its top-left corner; the pointer's own quarter is searched first, then the others in
/// order. In a quarter: the highest kind (see `Kind::rank`), then the nearest to the camera, then the first listed.
// ponytail: the other quarters go top-left, top-right, bottom-left, bottom-right, the best of the first with a hit.
pub fn pick<T: Copy>(targets: impl IntoIterator<Item = Target<T>>, cursor: [f32; 2], players_first: bool, middle: [f32; 2]) -> Option<T> {
    let [x, y] = cursor;
    let hits: Vec<Target<T>> =
        targets.into_iter().filter(|Target { area: [left, top, right, bottom], .. }| *left < x && x < *right && *top < y && y < *bottom).collect();
    let own = quarter(cursor, middle);
    [own].into_iter().chain((0..4).filter(|&other| other != own)).find_map(|searched| {
        hits.iter()
            .filter(|target| quarter([target.area[0], target.area[1]], middle) == searched)
            .min_by(|a, b| b.kind.rank(players_first).cmp(&a.kind.rank(players_first)).then(a.depth.total_cmp(&b.depth)))
            .map(|target| target.owner)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The middle of a 1024 by 768 screen.
    const MIDDLE: [f32; 2] = [512.0, 384.0];

    fn target(owner: u32, kind: Kind, sprite: Rect, depth: f32) -> Target<u32> {
        let min = if kind == Kind::Item { ITEM_MIN } else { UNIT_MIN };
        Target { owner, kind, sprite, area: area(sprite, min), depth }
    }

    fn pick_at(targets: &[Target<u32>], cursor: [f32; 2]) -> Option<u32> {
        pick(targets.iter().copied(), cursor, false, MIDDLE)
    }

    #[test]
    fn a_foe_on_a_siege_map_ranks_with_monsters() {
        let box_at = |x: f32| [x, 100.0, x + 70.0, 180.0];
        // Overlapping boxes, the pointer inside both: a guildmate loses to the monster, a foe wins by being nearer.
        let monster = target(1, Kind::Monster, box_at(100.0), 50.0);
        let guildmate = target(2, Kind::Player, box_at(110.0), 10.0);
        let foe = target(3, Kind::Foe, box_at(110.0), 10.0);
        assert_eq!(pick_at(&[monster, guildmate], [130.0, 140.0]), Some(1));
        assert_eq!(pick_at(&[monster, foe], [130.0, 140.0]), Some(3));
        assert_eq!(pick_at(&[monster, target(3, Kind::Foe, box_at(110.0), 90.0)], [130.0, 140.0]), Some(1));
    }

    #[test]
    fn small_sprites_grow_to_the_minimum_around_their_middle() {
        // A Poring at the default zoom, 50 by 45; zoomed out, 19 by 18; a potion, 26 by 28.
        assert_eq!(area([100.0, 100.0, 150.0, 145.0], UNIT_MIN), [93.0, 90.5, 157.0, 154.5]);
        assert_eq!(area([100.0, 100.0, 119.0, 118.0], UNIT_MIN), [77.5, 77.0, 141.5, 141.0]);
        assert_eq!(area([200.0, 200.0, 226.0, 228.0], ITEM_MIN), [186.0, 187.0, 240.0, 241.0]);
        // A tall NPC only grows sideways: its cell in front of its feet isn't part of it.
        assert_eq!(area([0.0, 0.0, 40.0, 100.0], UNIT_MIN), [-12.0, 0.0, 52.0, 100.0]);
        let npc = target(1, Kind::Npc, [100.0, 0.0, 140.0, 100.0], 1.0);
        assert_eq!((pick_at(&[npc], [120.0, 90.0]), pick_at(&[npc], [120.0, 105.0])), (Some(1), None));
    }

    #[test]
    fn counts_only_strictly_inside() {
        // The area runs from 93 to 157 across and 90.5 to 154.5 down: its edges don't count.
        let poring = target(1, Kind::Monster, [100.0, 100.0, 150.0, 145.0], 1.0);
        assert_eq!(pick_at(&[poring], [94.0, 120.0]), Some(1));
        assert_eq!((pick_at(&[poring], [93.0, 120.0]), pick_at(&[poring], [157.0, 120.0])), (None, None));
        assert_eq!((pick_at(&[poring], [120.0, 90.5]), pick_at(&[poring], [120.0, 154.5])), (None, None));
    }

    #[test]
    fn monsters_win_over_items_npcs_and_players_unless_players_come_first() {
        let monster = target(1, Kind::Monster, [100.0, 100.0, 150.0, 145.0], 9.0);
        let item = target(2, Kind::Item, [110.0, 130.0, 136.0, 158.0], 8.0);
        let npc = target(3, Kind::Npc, [90.0, 40.0, 140.0, 150.0], 7.0);
        let player = target(4, Kind::Player, [95.0, 30.0, 145.0, 151.0], 1.0);
        let all = [monster, item, npc, player];
        // Everyone is under the pointer: the monster, though the player is nearer the camera and the item is on top of it.
        assert_eq!(pick_at(&all, [120.0, 140.0]), Some(1));
        assert_eq!(pick_at(&[item, npc, player], [120.0, 140.0]), Some(2));
        assert_eq!(pick_at(&[npc, player], [120.0, 140.0]), Some(3));
        // Shift held: the player.
        assert_eq!(pick(all, [120.0, 140.0], true, MIDDLE), Some(4));
    }

    #[test]
    fn among_the_same_kind_the_nearest_wins_then_the_first_listed() {
        // Two Porings, the second a little behind and to the right: wherever their areas overlap, the front one.
        let front = target(1, Kind::Monster, [100.0, 100.0, 150.0, 145.0], 4.0);
        let back = target(2, Kind::Monster, [140.0, 90.0, 190.0, 135.0], 5.0);
        assert_eq!(pick_at(&[front, back], [153.0, 120.0]), Some(1));
        assert_eq!(pick_at(&[back, front], [145.0, 120.0]), Some(1));
        assert_eq!(pick_at(&[front, back], [170.0, 120.0]), Some(2), "only on the back one");
        // Just as near: the one listed first.
        let twin = target(3, Kind::Monster, [100.0, 100.0, 150.0, 145.0], 4.0);
        assert_eq!((pick_at(&[front, twin], [120.0, 120.0]), pick_at(&[twin, front], [120.0, 120.0])), (Some(1), Some(3)));
    }

    #[test]
    fn searches_the_pointers_quarter_first() {
        // A monster whose area starts left of the middle reaches across it; a player's starts right of it.
        let monster = target(1, Kind::Monster, [480.0, 100.0, 560.0, 180.0], 1.0);
        let player = target(2, Kind::Player, [520.0, 100.0, 600.0, 180.0], 9.0);
        // Right of the middle, the player's quarter goes first, though the monster ranks higher.
        assert_eq!(pick_at(&[monster, player], [540.0, 140.0]), Some(2));
        // Left of it, only the monster is there.
        assert_eq!(pick_at(&[monster, player], [500.0, 140.0]), Some(1));
        // With nothing in the pointer's quarter, another quarter's area still counts.
        assert_eq!(pick_at(&[monster], [540.0, 140.0]), Some(1));
    }

    #[test]
    fn a_warp_ranks_as_an_npc() {
        // A player standing on a warp: the warp, as an NPC, wins.
        let warp = target(1, Kind::Npc, [100.0, 120.0, 160.0, 160.0], 5.0);
        let player = target(2, Kind::Player, [105.0, 60.0, 155.0, 150.0], 4.0);
        assert_eq!(pick_at(&[player, warp], [130.0, 140.0]), Some(1));
    }
}
