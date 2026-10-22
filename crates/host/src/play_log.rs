//! What a web player's playing does, for the log (owner 2026-10-21: a player's Spotify skipped songs and fell
//! silent after ten seconds, his YouTube never played, and the log said nothing): each change of what it does
//! (loading, playing, paused, stopped, the next track), with where in the track it stopped. Never what
//! played: tracks are only compared, never written.

/// What a player does now, as its playback report says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Doing {
    Loading,
    Playing,
    Paused,
}

/// What a player does, from its report's loading and playing.
pub fn doing(loading: bool, playing: bool) -> Doing {
    if loading {
        Doing::Loading
    } else if playing {
        Doing::Playing
    } else {
        Doing::Paused
    }
}

/// One playback report, as the log needs it: what it does, the track (compared, never logged) and where in
/// it (seconds).
pub struct Heard<'a> {
    pub doing: Doing,
    pub track: Option<&'a str>,
    pub position: f64,
}

/// One player's log of its playing: the last it did, the track then, and where.
pub struct PlayLog {
    name: &'static str,
    last: Option<(Doing, Option<String>, f64)>,
}

impl PlayLog {
    pub fn new(name: &'static str) -> Self {
        Self { name, last: None }
    }

    /// A playback report (`None`: nothing to play, stopped): the line to log if what it does changed.
    pub fn heard(&mut self, now: Option<Heard>) -> Option<String> {
        let name = self.name;
        let line = match (&self.last, &now) {
            (None, None) => None,
            (Some((_, _, at)), None) => Some(format!("[{name}] stopped at {at:.0} s")),
            (last, Some(now)) => {
                let same_track = last.as_ref().is_some_and(|(_, track, _)| track.as_deref() == now.track);
                let doing = last.as_ref().map(|(doing, _, _)| *doing);
                match now.doing {
                    _ if !same_track && last.is_some() && now.track.is_some() => Some(match now.doing {
                        Doing::Playing => format!("[{name}] next track, playing"),
                        Doing::Loading => format!("[{name}] next track, loading"),
                        Doing::Paused => format!("[{name}] next track, paused"),
                    }),
                    current if doing == Some(current) => None,
                    Doing::Playing => Some(format!("[{name}] playing at {:.0} s", now.position)),
                    Doing::Loading => Some(format!("[{name}] loading at {:.0} s", now.position)),
                    Doing::Paused => Some(format!("[{name}] paused at {:.0} s", now.position)),
                }
            }
        };
        self.last = now.map(|now| (now.doing, now.track.map(str::to_owned), now.position));
        line
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn heard(doing: Doing, track: &str, position: f64) -> Option<Heard<'_>> {
        Some(Heard { doing, track: Some(track), position })
    }

    #[test]
    fn logs_each_change_of_what_it_does_never_what_played() {
        let mut log = PlayLog::new("spotify");
        assert_eq!(log.heard(heard(Doing::Loading, "a", 0.0)).as_deref(), Some("[spotify] loading at 0 s"));
        assert_eq!(log.heard(heard(Doing::Playing, "a", 0.2)).as_deref(), Some("[spotify] playing at 0 s"));
        // Playing on: nothing.
        assert_eq!(log.heard(heard(Doing::Playing, "a", 5.0)), None);
        assert_eq!(log.heard(heard(Doing::Paused, "a", 10.4)).as_deref(), Some("[spotify] paused at 10 s"));
        // Another track: said so, never which.
        assert_eq!(log.heard(heard(Doing::Playing, "b", 0.0)).as_deref(), Some("[spotify] next track, playing"));
        assert_eq!(log.heard(None).as_deref(), Some("[spotify] stopped at 0 s"));
        assert_eq!(log.heard(None), None);
    }

    #[test]
    fn a_track_that_falls_silent_shows_where() {
        let mut log = PlayLog::new("spotify");
        log.heard(heard(Doing::Playing, "a", 0.0));
        log.heard(heard(Doing::Playing, "a", 10.2));
        assert_eq!(log.heard(None).as_deref(), Some("[spotify] stopped at 10 s"));
    }

    #[test]
    fn a_first_report_without_a_track_is_what_it_does() {
        let mut log = PlayLog::new("youtube");
        let nothing = Heard { doing: Doing::Paused, track: None, position: 0.0 };
        assert_eq!(log.heard(Some(nothing)).as_deref(), Some("[youtube] paused at 0 s"));
    }
}
