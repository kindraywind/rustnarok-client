//! The player's homunculus: its window (Alt+R), its skill list, the words for its hunger and intimacy.

#[cfg(test)]
mod tests {
    use super::*;

    fn skill(id: u16, level: u16, upgradable: bool) -> Skill {
        Skill { id, kind: 0, level, sp: 0, range: 0, upgradable }
    }

    fn centre(rect: [f32; 4]) -> [f32; 2] {
        [rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0]
    }

    fn open_window() -> Window {
        let mut window = Window::default();
        window.frame.open = true;
        window
    }

    fn open_list() -> SkillList {
        let mut list = SkillList::default();
        list.frame.open = true;
        list
    }

    #[test]
    fn names_hunger_and_intimacy() {
        assert_eq!([0, 10, 11, 25, 26, 75, 76, 90, 91, 100].map(hunger_word), [667, 667, 668, 668, 669, 669, 670, 670, 671, 671]);
        assert_eq!(
            [0, 3, 4, 10, 11, 100, 101, 250, 251, 750, 751, 910, 911, 1000].map(intimacy_word),
            [1022, 1022, 1021, 1021, 672, 672, 673, 673, 669, 669, 674, 674, 675, 675]
        );
    }

    #[test]
    fn says_starving_once_per_crossing() {
        assert!(starving_now(Some(11), 10));
        assert!(!starving_now(Some(10), 9));
        assert!(starving_now(None, 5));
        assert!(!starving_now(Some(5), 50));
        assert!(!starving_now(None, 11));
    }

    #[test]
    fn hits_the_window_buttons() {
        let window = open_window();
        let homunculus = Homunculus::default();
        let boxes = window.button_boxes(&homunculus);
        assert_eq!(boxes.iter().map(|(name, _)| *name).collect::<Vec<_>>(), ["feed", "skill", "rename"]);
        for (name, rect) in &boxes {
            let expected = match *name {
                "feed" => Hit::Feed,
                "skill" => Hit::Skills,
                _ => Hit::Rename,
            };
            assert_eq!(window.hit(centre(*rect), &homunculus), Some(expected), "{name}");
        }
    }

    #[test]
    fn renaming_is_once() {
        let window = open_window();
        let renamed = Homunculus { flags: 0b001, ..Default::default() };
        let boxes = window.button_boxes(&renamed);
        assert_eq!(boxes.iter().map(|(name, _)| *name).collect::<Vec<_>>(), ["feed", "skill"]);
        let unnamed = window.button_boxes(&Homunculus::default());
        let rename = unnamed.iter().find(|(name, _)| *name == "rename").unwrap().1;
        assert_ne!(window.hit(centre(rename), &renamed), Some(Hit::Rename));
    }

    #[test]
    fn a_closed_window_has_no_boxes() {
        let window = Window::default();
        assert!(window.button_boxes(&Homunculus::default()).is_empty());
    }

    #[test]
    fn hits_the_skill_list() {
        let list = open_list();
        let skills = [skill(8001, 3, true), skill(8002, 1, false), skill(8003, 1, true)];
        let boxes = list.skill_boxes(&skills);
        assert_eq!(boxes.iter().map(|(id, _)| *id).collect::<Vec<_>>(), [8001, 8002, 8003]);
        for (id, rect) in &boxes {
            assert_eq!(list.hit(centre(*rect), &skills, 0), Some(SkillHit::Skill(*id)));
        }
        let raises = list.raise_boxes(&skills, 2);
        assert_eq!(raises.iter().map(|(id, _)| *id).collect::<Vec<_>>(), [8001, 8003]);
        for (id, rect) in &raises {
            assert_eq!(list.hit(centre(*rect), &skills, 2), Some(SkillHit::Raise(*id)));
        }
    }

    #[test]
    fn raises_need_points() {
        let list = open_list();
        let skills = [skill(8001, 3, true), skill(8002, 1, false)];
        assert!(list.raise_boxes(&skills, 0).is_empty());
        assert!(list.raise_boxes(&[skill(8002, 1, false)], 5).is_empty());
    }

    #[test]
    fn a_click_outside_the_list_hits_nothing() {
        let list = open_list();
        let skills = [skill(8001, 3, true)];
        assert_eq!(list.hit([-100.0, -100.0], &skills, 1), None);
    }
}
