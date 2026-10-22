//! The server's walk search on a map's GAT cells: the path a walk takes, a unit's walk heard from the server, and the
//! navigation trail.

use std::collections::HashMap;

use crate::map::Gat;

/// A walk's path: the search with the game's 149-node limit.
pub fn find_path(gat: &Gat, from: [i32; 2], to: [i32; 2]) -> Option<Vec<[i32; 2]>> {
    search(gat, from, to, Some(WALK_NODES), None)
}

/// A unit's walk heard from the server, by its object type: an NPC with a monster's look (12) may step onto type-5
/// cells.
pub fn find_unit_path(gat: &Gat, from: [i32; 2], to: [i32; 2], kind: u8) -> Option<Vec<[i32; 2]>> {
    search(gat, from, to, Some(WALK_NODES), Some(kind))
}

/// rathena's object type for an NPC whose look is a monster class (`clif_bl_type`).
const MONSTER_LOOK: u8 = 12;

/// The navigation trail's path: the same search with no limit.
pub fn find_trail(gat: &Gat, from: [i32; 2], to: [i32; 2]) -> Option<Vec<[i32; 2]>> {
    search(gat, from, to, None, None)
}

/// The most cells a walk's search reaches: the start is the first, and the 150th ends it with no path.
const WALK_NODES: usize = 149;

/// Cells walked from `from` to `to` (excluding `from`), 8-directional; diagonals need both sides open. The server's
/// search, step for step, so both walk the same cells: A* at 10 a straight step and 14 a diagonal, with 10 times the
/// Manhattan distance left as the estimate, neighbours tried from south-east round anticlockwise, and a binary heap on
/// that sum alone, ties falling where the heap leaves them. A walk across open ground takes its diagonal steps first.
/// At most `limit` cells are reached; there's no path from a cell to itself. The walker's object type (`kind`) lets an
/// NPC with a monster's look step onto type-5 cells; a diagonal's side cells still block on 1 and 5.
fn search(gat: &Gat, from: [i32; 2], to: [i32; 2], limit: Option<usize>, kind: Option<u8>) -> Option<Vec<[i32; 2]>> {
    struct Node {
        cell: [i32; 2],
        parent: Option<usize>,
        cost: i32,
        sum: i32,
        open: bool,
    }
    if from == to {
        return None;
    }
    let monster_look = kind == Some(MONSTER_LOOK);
    if !gat.walkable_by(to, monster_look) {
        return None;
    }
    let estimate = |cell: [i32; 2]| 10 * ((cell[0] - to[0]).abs() + (cell[1] - to[1]).abs());
    let mut nodes = vec![Node { cell: from, parent: None, cost: 0, sum: estimate(from), open: true }];
    let mut known = HashMap::from([(from, 0)]);
    let mut heap = vec![0];
    while !heap.is_empty() {
        let current = heap_pop(&mut heap, &nodes, |node| node.sum);
        nodes[current].open = false;
        let (cell, spent) = (nodes[current].cell, nodes[current].cost);
        if cell == to {
            let mut path = vec![];
            let mut at = current;
            while let Some(parent) = nodes[at].parent {
                path.push(nodes[at].cell);
                at = parent;
            }
            path.reverse();
            return Some(path);
        }
        // A diagonal's side cells block on 1 and 5; the cell stepped onto takes the walker's own test.
        let open = |dx: i32, dy: i32| gat.walkable([cell[0] + dx, cell[1] + dy]);
        let onto = |dx: i32, dy: i32| gat.walkable_by([cell[0] + dx, cell[1] + dy], monster_look);
        let (north, south, east, west) = (open(0, 1), open(0, -1), open(1, 0), open(-1, 0));
        let ways = [
            (1, -1, south && east),
            (1, 0, true),
            (1, 1, north && east),
            (0, 1, true),
            (-1, 1, north && west),
            (-1, 0, true),
            (-1, -1, south && west),
            (0, -1, true),
        ];
        for (dx, dy, sides) in ways {
            if !sides || !onto(dx, dy) {
                continue;
            }
            let diagonal = dx != 0 && dy != 0;
            let next = [cell[0] + dx, cell[1] + dy];
            let cost = spent + if diagonal { 14 } else { 10 };
            match known.get(&next) {
                Some(&index) if cost < nodes[index].cost => {
                    let node = &mut nodes[index];
                    (node.cost, node.parent, node.sum) = (cost, Some(current), cost + estimate(next));
                    if node.open {
                        let at = heap.iter().position(|&queued| queued == index).expect("an open node is queued");
                        heap_rise(&mut heap, &nodes, |node| node.sum, 0, at);
                        heap_sink(&mut heap, &nodes, |node| node.sum, at);
                    } else {
                        node.open = true;
                        heap_push(&mut heap, &nodes, |node| node.sum, index);
                    }
                }
                Some(_) => {}
                // A cell reached the first time is a new node; one past the limit ends the search with no path.
                None => {
                    if limit.is_some_and(|limit| nodes.len() >= limit) {
                        return None;
                    }
                    known.insert(next, nodes.len());
                    nodes.push(Node { cell: next, parent: Some(current), cost, sum: cost + estimate(next), open: true });
                    heap_push(&mut heap, &nodes, |node| node.sum, nodes.len() - 1);
                }
            }
        }
    }
    None
}

// The server's binary heap of indices into `items`, smallest `key` on top; its exact moves decide which of equal keys
// comes out first.
fn heap_push<T>(heap: &mut Vec<usize>, items: &[T], key: impl Fn(&T) -> i32, item: usize) {
    heap.push(item);
    let last = heap.len() - 1;
    heap_rise(heap, items, key, 0, last);
}

/// Takes the top off: the last item goes on top and sinks to a leaf, then rises.
fn heap_pop<T>(heap: &mut Vec<usize>, items: &[T], key: impl Fn(&T) -> i32) -> usize {
    let top = heap.swap_remove(0);
    if !heap.is_empty() {
        heap_sink(heap, items, key, 0);
    }
    top
}

/// Moves the item at `at` up while its parent's key is larger, no higher than `top`.
fn heap_rise<T>(heap: &mut [usize], items: &[T], key: impl Fn(&T) -> i32, top: usize, mut at: usize) {
    while at > top {
        let parent = (at - 1) / 2;
        if key(&items[heap[parent]]) <= key(&items[heap[at]]) {
            break;
        }
        heap.swap(parent, at);
        at = parent;
    }
}

/// Moves the item at `from` down to a leaf, always by the smaller child (the right one on a tie), then back up.
fn heap_sink<T>(heap: &mut [usize], items: &[T], key: impl Fn(&T) -> i32, from: usize) {
    let mut at = from;
    while 2 * at + 1 < heap.len() {
        let (left, right) = (2 * at + 1, 2 * at + 2);
        let child = if right >= heap.len() || key(&items[heap[left]]) < key(&items[heap[right]]) { left } else { right };
        heap.swap(at, child);
        at = child;
    }
    heap_rise(heap, items, key, from, at);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walks_go_round_an_ice_wall_while_it_stands() {
        // Open ground, 7 wide; a wall of three cells across x = 3 (y 1 … 3) set to type 5 and back.
        let mut gat = Gat { width: 7, height: 5, cells: vec![([0.0; 4], 0); 35] };
        let straight = find_unit_path(&gat, [0, 2], [6, 2], 5).unwrap();
        assert!(straight.contains(&[3, 2]));
        for y in 1..=3 {
            gat.set_kind([3, y], 5);
        }
        let round = find_unit_path(&gat, [0, 2], [6, 2], 5).unwrap();
        assert!(round.iter().all(|cell| !(cell[0] == 3 && (1..=3).contains(&cell[1]))), "{round:?}");
        assert_eq!(round.last(), Some(&[6, 2]));
        // Gone again: straight through.
        for y in 1..=3 {
            gat.set_kind([3, y], 0);
        }
        assert_eq!(find_unit_path(&gat, [0, 2], [6, 2], 5).unwrap(), straight);
        // A cell off the map is left alone.
        gat.set_kind([9, 9], 5);
    }

    #[test]
    fn paths_avoid_walls_and_cut_no_corners() {
        // 0 walkable, 1 blocked; y grows upward in GAT, rows listed bottom first.
        let rows = ["00000", "01110", "00010", "00000"];
        let cells = rows.iter().flat_map(|row| row.bytes().map(|byte| ([0.0; 4], if byte == b'0' { 0 } else { 1 }))).collect();
        let gat = Gat { width: 5, height: 4, cells };
        let path = find_path(&gat, [0, 0], [2, 2]).unwrap();
        assert_eq!(path.last(), Some(&[2, 2]));
        assert!(path.iter().all(|&cell| gat.walkable(cell)));
        let mut previous = [0, 0];
        for &cell in &path {
            let step = [cell[0] - previous[0], cell[1] - previous[1]];
            assert!(step[0].abs() <= 1 && step[1].abs() <= 1);
            if step[0] != 0 && step[1] != 0 {
                assert!(gat.walkable([previous[0] + step[0], previous[1]]) && gat.walkable([previous[0], previous[1] + step[1]]));
            }
            previous = cell;
        }
        assert!(find_path(&gat, [0, 0], [1, 1]).is_none());
    }

    #[test]
    fn monster_looking_npcs_step_onto_type_5() {
        // Cell (x, y) is cells[y * W + x]; the listed cells are type 5, the rest 0.
        let gat = |width: usize, height: usize, five: &[[usize; 2]]| {
            let mut cells = vec![([0.0; 4], 0); width * height];
            for [x, y] in five {
                cells[y * width + x].1 = 5;
            }
            Gat { width, height, cells }
        };
        let across = gat(3, 1, &[[1, 0]]);
        assert_eq!(find_unit_path(&across, [0, 0], [2, 0], 12), Some(vec![[1, 0], [2, 0]]));
        assert_eq!(find_unit_path(&across, [0, 0], [2, 0], 6), None);
        let goal = gat(3, 1, &[[2, 0]]);
        assert_eq!(find_unit_path(&goal, [0, 0], [2, 0], 12), Some(vec![[1, 0], [2, 0]]));
        assert_eq!(find_unit_path(&goal, [0, 0], [2, 0], 6), None);
        // The diagonal past a type-5 side cell stays shut, so kind 12 goes through it and kind 6 round it.
        let corner = gat(2, 2, &[[1, 0]]);
        assert_eq!(find_unit_path(&corner, [0, 0], [1, 1], 12), Some(vec![[1, 0], [1, 1]]));
        assert_eq!(find_unit_path(&corner, [0, 0], [1, 1], 6), Some(vec![[0, 1], [1, 1]]));
        // Type 1 blocks everyone; clicks and the trail (no kind) treat 5 as blocked.
        let mut wall = gat(3, 1, &[]);
        wall.cells[1].1 = 1;
        assert_eq!(find_unit_path(&wall, [0, 0], [2, 0], 12), None);
        assert_eq!(find_path(&across, [0, 0], [2, 0]), None);
    }

    #[test]
    fn walk_searches_stop_at_149_cells() {
        let open = Gat { width: 120, height: 120, cells: vec![([0.0; 4], 0); 120 * 120] };
        assert_eq!(find_path(&open, [60, 60], [107, 60]).map(|path| path.len()), Some(47));
        assert_eq!(find_path(&open, [60, 60], [108, 60]), None);
        assert_eq!(find_path(&open, [40, 40], [69, 69]).map(|path| path.len()), Some(29));
        assert_eq!(find_path(&open, [40, 40], [70, 70]), None);
        // The trail has no limit.
        assert_eq!(find_trail(&open, [60, 60], [108, 60]).map(|path| path.len()), Some(48));
        // A wall across rows: round it is too far for a walk, not for the trail.
        let mut cells = vec![([0.0; 4], 0); 27 * 12];
        for x in 3..=23 {
            cells[5 * 27 + x].1 = 1;
        }
        let walled = Gat { width: 27, height: 12, cells };
        assert_eq!(find_path(&walled, [13, 3], [13, 7]), None);
        assert!(find_trail(&walled, [13, 3], [13, 7]).is_some_and(|path| path.last() == Some(&[13, 7])));
        // No path from a cell to itself.
        assert_eq!(find_path(&open, [5, 5], [5, 5]), None);
        assert_eq!(find_trail(&open, [5, 5], [5, 5]), None);
    }

    #[test]
    fn paths_are_the_servers() {
        // The server's own search gives these (rows listed bottom first): on open ground the diagonal steps come
        // first, and round a wall the ties fall as its heap leaves them.
        let gat = |rows: &[&str]| Gat {
            width: rows[0].len(),
            height: rows.len(),
            cells: rows.iter().flat_map(|row| row.bytes().map(|byte| ([0.0; 4], if byte == b'0' { 0 } else { 1 }))).collect(),
        };
        let open = gat(&["0000000000"; 8]);
        assert_eq!(find_path(&open, [1, 1], [7, 5]).unwrap(), [[2, 2], [3, 3], [4, 4], [5, 5], [6, 5], [7, 5]]);
        let walled = gat(&["0000000000", "0000000000", "0001110000", "0000010000", "0000010000", "0000000000", "0000000000", "0000000000"]);
        assert_eq!(find_path(&walled, [1, 1], [8, 4]).unwrap(), [[2, 1], [3, 1], [4, 1], [5, 1], [6, 1], [7, 2], [8, 3], [8, 4]]);
    }
}
