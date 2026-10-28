//! Whether the game's camera is outside the cab, for YouTube's sound to come muffled through it.
//!
//! The SDK doesn't say which camera is on. But with the camera in the cab, the game draws its screen's
//! picture every frame, wherever the player looks (even back to the bed): the plugin sees WestPlay's screen
//! texture used ([`westplay_shared::PluginBlock::screen_frame`] moving on with the game's frames). From
//! outside, it isn't drawn: a few frames without it, and the camera is out (as quick as the game's cut from
//! one camera to another). Probed in the game 2026-10-14.
//!
//! WestPlay's tablet isn't such a screen: the game draws it with the cab's models, and leaves it out as the
//! player looks away from it, the camera still in the cab (owner 2026-10-25: the music muffled as the head turned
//! in the interior). So the cab's dashboard is drawn with a mark of WestPlay's in it too
//! (`westplay_shared::CAB_MARK_WIDTH`), drawn as the GPS screens are: the cab is drawn when either was, last.
//!
//! How far out it is: the plugin reads the camera's world position from the game; its distance from the
//! driver's head, where the SDK puts it in the truck (where the cab turns, plus the head in the cab), the
//! truck turned and moved as it is now. (Not learnt from the camera while it's in the cab: the screen is
//! drawn now and then with the camera outside too, from some angles through the windows.)

/// How many game frames in a row the cab's screen goes undrawn before the camera counts as outside (a frame
/// or two may pass without it in the cab as well).
pub const OUTSIDE_AFTER_FRAMES: u64 = 4;

#[derive(Default)]
pub struct Camera {
    truck: String,
    frame: u64,
    /// Whether the screen has been seen drawn in this truck.
    seen: bool,
    /// The answer last given: kept while the game is paused.
    outside: bool,
}

impl Camera {
    /// Whether the camera is outside the cab now: in this truck (its SDK id) the cab was drawn before but not in
    /// the last frames (`screen_frame`: the game frame its screen or its mark was last drawn in, [`drawn_frame`];
    /// `game_frame`: the game's frame now), while it would be (the electrics on). A truck with neither never
    /// counts as outside. While the game is paused, what it draws says nothing of the camera: its menu hides the
    /// cab, and switching between its menus can draw the cab with the camera outside (a player's video,
    /// 2026-10-28: the music came clear for a moment, as if in the cab). So the camera stays where it was as the
    /// game paused (owner 2026-10-26: outside, the music came clear as it paused) until the game runs again.
    pub fn outside(&mut self, truck: &str, screen_frame: u64, game_frame: u64, electrics: bool, paused: bool) -> bool {
        if self.truck != truck {
            self.truck = truck.to_owned();
            self.seen = false;
            self.frame = screen_frame;
            self.outside = false;
        }
        if screen_frame != self.frame {
            self.frame = screen_frame;
            self.seen = true;
        }
        let undrawn = game_frame.saturating_sub(screen_frame) > OUTSIDE_AFTER_FRAMES;
        if !paused {
            self.outside = self.seen && electrics && undrawn;
        }
        self.outside
    }

    /// With the camera outside, how far it is from the driver's head (metres); `None` with it inside, or the
    /// camera not known (the plugin couldn't find it in the game).
    pub fn distance(telemetry: &westplay_shared::Telemetry, outside: bool) -> Option<f64> {
        if !outside || !telemetry.camera_known {
            return None;
        }
        let truck = Pose::of(telemetry);
        let camera = [telemetry.camera_x as f64, telemetry.camera_y as f64, telemetry.camera_z as f64];
        let head = [
            telemetry.cabin_x as f64 + telemetry.head_x as f64,
            telemetry.cabin_y as f64 + telemetry.head_y as f64,
            telemetry.cabin_z as f64 + telemetry.head_z as f64,
        ];
        let seat = truck.to_world(head);
        Some(camera.iter().zip(seat).map(|(c, s)| (c - s).powi(2)).sum::<f64>().sqrt())
    }
}

/// While the game is paused: whether its menu has gone, the still picture shown before the truck moves on.
/// The game says it runs again only some 1.5 s after its menu goes (measured 2026-10-21: Esc, the cab drawn
/// again half a second later, the game running 1.5 s after that); the music should be back as the picture is
/// (owner 2026-10-21). With the camera in the cab, the menu hides the cab (its screen not drawn for a few
/// frames) and its going shows it again; drawn again after being hidden in this pause, the menu has gone.
/// Opened again, it hasn't. With the camera outside, never: the music then waits for the game to run again
/// (the cab can be drawn with it outside, switching between the game's menus: a player's video, 2026-10-28).
#[derive(Default)]
pub struct MenuGone {
    hidden: bool,
    gone: bool,
}

impl MenuGone {
    /// Whether the menu has gone now (`paused`: the game's; `drawn`: the cab's screen drawn in the last frames;
    /// `outside`: the camera, as [`Camera::outside`] holds it through the pause).
    pub fn update(&mut self, paused: bool, drawn: bool, outside: bool) -> bool {
        if !paused {
            *self = Self::default();
        } else if outside {
            self.gone = false;
        } else if !drawn {
            (self.hidden, self.gone) = (true, false);
        } else if self.hidden {
            self.gone = true;
        }
        self.gone
    }
}

/// The game frame the cab was last drawn in: WestPlay's screen's (`screen_frame`) or its mark's (`cab_frame`),
/// whichever was later.
pub fn drawn_frame(screen_frame: u64, cab_frame: u64) -> u64 {
    screen_frame.max(cab_frame)
}

/// Whether the cab's screen was drawn in the last frames (see [`OUTSIDE_AFTER_FRAMES`]).
pub fn drawn(screen_frame: u64, game_frame: u64) -> bool {
    game_frame.saturating_sub(screen_frame) <= OUTSIDE_AFTER_FRAMES
}

/// The truck's place and turn in the world (the SDK's: heading round the up axis, counterclockwise seen from
/// above, 0 forward, which is -z; pitch up; roll counterclockwise looking forward; all in turns).
struct Pose {
    at: [f64; 3],
    heading: f64,
    pitch: f64,
    roll: f64,
}

impl Pose {
    fn of(telemetry: &westplay_shared::Telemetry) -> Self {
        let turn = |t: f32| t as f64 * std::f64::consts::TAU;
        Self {
            at: [telemetry.x, telemetry.y, telemetry.z],
            heading: turn(telemetry.heading),
            pitch: turn(telemetry.pitch),
            roll: turn(telemetry.roll),
        }
    }

    /// A point of the truck (its own space: x right, y up, z back) in the world: rolled, pitched, headed,
    /// moved to it.
    fn to_world(&self, [x, y, z]: [f64; 3]) -> [f64; 3] {
        let (x, y) = (x * self.roll.cos() - y * self.roll.sin(), x * self.roll.sin() + y * self.roll.cos());
        let (y, z) = (y * self.pitch.cos() - z * self.pitch.sin(), y * self.pitch.sin() + z * self.pitch.cos());
        let (x, z) =
            (x * self.heading.cos() + z * self.heading.sin(), -x * self.heading.sin() + z * self.heading.cos());
        [x + self.at[0], y + self.at[1], z + self.at[2]]
    }

    /// A point in the world in the truck's own space: [`Pose::to_world`] undone.
    #[cfg(test)]
    fn to_truck(&self, point: [f64; 3]) -> [f64; 3] {
        let [x, y, z] = [point[0] - self.at[0], point[1] - self.at[1], point[2] - self.at[2]];
        let (x, z) = (x * self.heading.cos() - z * self.heading.sin(), x * self.heading.sin() + z * self.heading.cos());
        let (y, z) = (y * self.pitch.cos() + z * self.pitch.sin(), -y * self.pitch.sin() + z * self.pitch.cos());
        let (x, y) = (x * self.roll.cos() + y * self.roll.sin(), -x * self.roll.sin() + y * self.roll.cos());
        [x, y, z]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menu_has_gone_once_the_cab_shows_again_in_its_pause() {
        let mut menu = MenuGone::default();
        // Paused: the cab still drawn a moment, then hidden by the menu.
        assert!(!menu.update(true, true, false));
        assert!(!menu.update(true, false, false));
        // The menu goes: the cab drawn again, the game still paused.
        assert!(menu.update(true, true, false));
        // Opened again before the game ran: not gone.
        assert!(!menu.update(true, false, false));
        assert!(menu.update(true, true, false));
        // The game runs: over.
        assert!(!menu.update(false, true, false));
        // Paused with the camera outside: the cab undrawn, and drawn again as the game switches between its
        // menus (a player's video, 2026-10-28): never gone.
        assert!(!menu.update(true, false, true));
        assert!(!menu.update(true, true, true));
        assert!(!menu.update(true, false, true));
        assert!(!menu.update(true, true, true));
    }

    fn close(a: [f64; 3], b: [f64; 3]) -> bool {
        a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-6)
    }

    #[test]
    fn turns_a_trucks_point_into_the_world_as_the_sdk_turns_it_and_back() {
        // Heading a quarter turn: forward (-z) is to the left (-x), as the SDK says.
        let left = Pose { at: [0.0; 3], heading: std::f64::consts::FRAC_PI_2, pitch: 0.0, roll: 0.0 };
        assert!(close(left.to_world([0.0, 0.0, -1.0]), [-1.0, 0.0, 0.0]));
        // Pitched up: forward rises.
        let up = Pose { at: [0.0; 3], heading: 0.0, pitch: 0.3, roll: 0.0 };
        let forward = up.to_world([0.0, 0.0, -1.0]);
        assert!(forward[1] > 0.29 && forward[2] < -0.9);
        // Any pose: there and back is where it started.
        let pose = Pose { at: [25825.7, 19.5, 57950.0], heading: 2.1, pitch: -0.05, roll: 0.02 };
        let point = [-0.7, 2.6, -1.9];
        assert!(close(pose.to_truck(pose.to_world(point)), point));
    }

    #[test]
    fn measures_the_cameras_distance_from_the_drivers_head_as_the_truck_turns() {
        // The DAF, as the game had it: the cab turning at (0, 1.02, -2.59), the head at (-0.72, 1.66, 0.78)
        // in the cab: in the truck, 0.72 m left, 2.68 m up, 1.81 m forward (where the camera was, in the cab).
        let mut telemetry = westplay_shared::Telemetry {
            x: 1000.0,
            z: 2000.0,
            heading: 0.687,
            camera_known: true,
            cabin_y: 1.02,
            cabin_z: -2.59,
            head_x: -0.72,
            head_y: 1.66,
            head_z: 0.78,
            ..Default::default()
        };
        // The chase camera 15 m behind and up, as probed: about 15 m from the head.
        let at = |telemetry: &westplay_shared::Telemetry, point: [f64; 3]| {
            Pose::of(telemetry).to_world(point).map(|v| v as f32)
        };
        [telemetry.camera_x, telemetry.camera_y, telemetry.camera_z] = at(&telemetry, [0.0, 6.13, 12.77]);
        let far = Camera::distance(&telemetry, true).unwrap();
        assert!((far - 15.0).abs() < 0.1, "{far}");
        // Zoomed in: nearer.
        [telemetry.camera_x, telemetry.camera_y, telemetry.camera_z] = at(&telemetry, [0.0, 3.66, 5.16]);
        let near = Camera::distance(&telemetry, true).unwrap();
        assert!(near < far && (near - 7.1).abs() < 0.1, "{near}");
        // The truck turned a quarter and moved on, the camera in front of the cab, as near: as near.
        telemetry.heading += 0.25;
        telemetry.x += 30.0;
        [telemetry.camera_x, telemetry.camera_y, telemetry.camera_z] = at(&telemetry, [-0.72, 2.68, -8.91]);
        let front = Camera::distance(&telemetry, true).unwrap();
        assert!((front - 7.1).abs() < 0.1, "{front}");
        // The camera in the cab, or not known: no distance.
        assert_eq!(Camera::distance(&telemetry, false), None);
        telemetry.camera_known = false;
        assert_eq!(Camera::distance(&telemetry, true), None);
    }

    const S: &str = "vehicle.scania.s_2016";

    /// The camera seen every game frame for `frames` frames, the screen drawn in each (or not); whether it
    /// ended outside.
    fn watch(camera: &mut Camera, game: &mut u64, screen: &mut u64, frames: u64, drawn: bool) -> bool {
        let mut outside = false;
        for _ in 0..frames {
            *game += 1;
            if drawn {
                *screen = *game;
            }
            outside = camera.outside(S, *screen, *game, true, false);
        }
        outside
    }

    #[test]
    fn is_outside_a_few_frames_after_the_cabs_screen_goes_undrawn_and_inside_as_soon_as_its_drawn() {
        let (mut camera, mut game, mut screen) = (Camera::default(), 100, 0);
        assert!(!watch(&mut camera, &mut game, &mut screen, 120, true));
        // Out: not on the first frames, then for as long as it lasts.
        assert!(!watch(&mut camera, &mut game, &mut screen, OUTSIDE_AFTER_FRAMES, false));
        assert!(watch(&mut camera, &mut game, &mut screen, 1, false));
        assert!(watch(&mut camera, &mut game, &mut screen, 300, false));
        // Back in: the first frame drawn.
        assert!(!watch(&mut camera, &mut game, &mut screen, 1, true));
        // A frame or two without it, in the cab: still in.
        assert!(!watch(&mut camera, &mut game, &mut screen, 2, false));
    }

    #[test]
    fn is_in_the_cab_while_its_mark_is_drawn_with_the_tablet_looked_away_from() {
        let (mut camera, mut game, mut screen, mut mark) = (Camera::default(), 100, 0, 0);
        // The tablet and the dashboard drawn: in.
        for _ in 0..60 {
            game += 1;
            (screen, mark) = (game, game);
            assert!(!camera.outside(S, drawn_frame(screen, mark), game, true, false));
        }
        // The head turned away from the tablet: the dashboard still drawn every frame, in.
        for _ in 0..300 {
            game += 1;
            mark = game;
            assert!(!camera.outside(S, drawn_frame(screen, mark), game, true, false));
        }
        // An outside camera: neither drawn, out after a few frames.
        game += OUTSIDE_AFTER_FRAMES + 1;
        assert!(camera.outside(S, drawn_frame(screen, mark), game, true, false));
    }

    #[test]
    fn stays_where_it_was_while_the_game_is_paused() {
        let (mut camera, mut game, mut screen) = (Camera::default(), 100, 0);
        // Outside, then paused: the menu up (the cab undrawn either way), still outside, however long.
        watch(&mut camera, &mut game, &mut screen, 60, true);
        assert!(watch(&mut camera, &mut game, &mut screen, 10, false));
        for _ in 0..300 {
            game += 1;
            assert!(camera.outside(S, screen, game, true, true));
        }
        // Run again: still outside.
        assert!(watch(&mut camera, &mut game, &mut screen, 1, false));
        // In the cab, then paused: the menu hides the cab, still in.
        assert!(!watch(&mut camera, &mut game, &mut screen, 60, true));
        for _ in 0..300 {
            game += 1;
            assert!(!camera.outside(S, screen, game, true, true));
        }
        // Paused outside, the cab drawn as the game switches between its menus (a player's video, 2026-10-28):
        // still outside, the whole pause.
        assert!(watch(&mut camera, &mut game, &mut screen, 10, false));
        for drawn in [true, true, false, true, false, false, true] {
            game += 1;
            if drawn {
                screen = game;
            }
            assert!(camera.outside(S, screen, game, true, true));
        }
        // Run again, the cab drawn: in, at once.
        assert!(!watch(&mut camera, &mut game, &mut screen, 1, true));
    }

    #[test]
    fn never_outside_without_the_screen_to_go_by() {
        let mut camera = Camera::default();
        // A truck with no WestPlay screen: never drawn, never outside.
        camera.outside("vehicle.man.tgx", 0, 10, true, false);
        assert!(!camera.outside("vehicle.man.tgx", 0, 500, true, false));
        // The electrics off or the game paused: the screen isn't drawn for those.
        camera.outside(S, 1, 1, true, false);
        camera.outside(S, 2, 2, true, false);
        assert!(!camera.outside(S, 2, 500, false, false));
        assert!(!camera.outside(S, 2, 500, true, true));
        assert!(camera.outside(S, 2, 500, true, false));
        // Another truck starts over: inside until its screen has been seen.
        assert!(!camera.outside("vehicle.daf.2021", 2, 900, true, false));
    }
}
