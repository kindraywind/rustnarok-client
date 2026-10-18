//! The level up buttons left in the screen's bottom corners: base level at the right (it opens the status window),
//! job level at the left (the skill window). Each stays, and marks its menu button new, until its window opens.

use rustnarok_client::grf::Grf;
use rustnarok_client::net::{STATUS_BASE_LEVEL, STATUS_JOB_LEVEL};

use crate::render::{Gpu, Texture};
use crate::{Layer, SCREEN, UI, contains};

const SIZE: [f32; 2] = [43.0, 43.0];

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Button {
    /// Base level: opens the status window.
    Base,
    /// Job level: opens the skill window.
    Job,
}

impl Button {
    pub fn corner(self) -> [f32; 2] {
        match self {
            Self::Base => [SCREEN[0] - SIZE[0], SCREEN[1] - SIZE[1]],
            Self::Job => [0.0, SCREEN[1] - SIZE[1]],
        }
    }
}

/// Which buttons are up.
#[derive(Default, Debug, PartialEq)]
pub struct LevelUps {
    pub base: bool,
    pub job: bool,
}

impl LevelUps {
    /// A status changed from `before` (None: not known yet, as at login) to `now`: a level that rose leaves its button.
    pub fn changed(&mut self, id: u16, before: Option<i64>, now: i64) {
        let rose = before.is_some_and(|before| now > before);
        match id {
            STATUS_BASE_LEVEL if rose => self.base = true,
            STATUS_JOB_LEVEL if rose => self.job = true,
            _ => {}
        }
    }

    /// An open status (skill) window has seen the base (job) level.
    pub fn seen(&mut self, status_open: bool, skills_open: bool) {
        self.base &= !status_open;
        self.job &= !skills_open;
    }

    fn shown(&self) -> impl Iterator<Item = Button> + '_ {
        [(Button::Base, self.base), (Button::Job, self.job)].into_iter().filter(|(_, up)| *up).map(|(button, _)| button)
    }

    pub fn hit(&self, cursor: [f32; 2]) -> Option<Button> {
        self.shown().find(|button| contains(button.corner(), SIZE, cursor))
    }

    /// The buttons on screen (for the test driver): `level_up_base`, `level_up_job`.
    pub fn button_boxes(&self) -> Vec<(&'static str, [f32; 4])> {
        self.shown()
            .map(|button| {
                let [x, y] = button.corner();
                (if button == Button::Base { "level_up_base" } else { "level_up_job" }, [x, y, SIZE[0], SIZE[1]])
            })
            .collect()
    }

    pub fn draw<'a>(&self, layer: &mut Layer<'a>, assets: &'a Assets, (pointer, down): ([f32; 2], bool)) {
        for button in self.shown() {
            let look = &assets.looks[usize::from(down && contains(button.corner(), SIZE, pointer))];
            layer.sprites.push((look, button.corner(), SIZE));
        }
    }
}

pub struct Assets {
    /// Out and pressed.
    looks: [Texture; 2],
}

impl Assets {
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        let image = |name: &str| gpu.try_load_texture(grf, &format!("{UI}basic_interface\\{name}.bmp"), false).unwrap_or_else(|_| gpu.solid([0, 0, 0, 0]));
        Self { looks: [image("lv_up_off"), image("lv_up_on")] }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stays_until_its_window_opens() {
        let mut ups = LevelUps::default();
        // Levels arriving at login don't count; a level that rises does.
        ups.changed(STATUS_BASE_LEVEL, None, 10);
        assert_eq!(ups, LevelUps::default());
        ups.changed(STATUS_BASE_LEVEL, Some(10), 11);
        ups.changed(STATUS_JOB_LEVEL, Some(5), 5);
        assert_eq!(ups, LevelUps { base: true, job: false });
        ups.changed(STATUS_JOB_LEVEL, Some(5), 6);
        // The skill window seen: only the job button goes.
        ups.seen(false, true);
        assert_eq!(ups, LevelUps { base: true, job: false });
        assert_eq!(ups.hit([1000.0, 750.0]), Some(Button::Base));
        assert_eq!(ups.hit([10.0, 750.0]), None);
        assert_eq!(ups.button_boxes(), [("level_up_base", [981.0, 725.0, 43.0, 43.0])]);
        ups.seen(true, false);
        assert_eq!(ups, LevelUps::default());
    }
}
