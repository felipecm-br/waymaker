use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy)]
pub struct Spinner {
    pub frames: &'static [&'static str],
    pub fps: u8,
}

impl Spinner {
    pub const DOT: Spinner = Spinner {
        frames: &["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"],
        fps: 10,
    };
    pub const LINE: Spinner = Spinner {
        frames: &["|", "/", "-", "\\"],
        fps: 10,
    };
    pub const JUMP: Spinner = Spinner {
        frames: &["⢄", "⢂", "⢁", "⡁", "⡈", "⡐", "⡠"],
        fps: 10,
    };
    pub const PULSE: Spinner = Spinner {
        frames: &["█", "▓", "▒", "░"],
        fps: 10,
    };
    pub const POINTS: Spinner = Spinner {
        frames: &["∙∙∙", "●∙∙", "∙●∙", "∙∙●", "∙∙∙"],
        fps: 7,
    };
    pub const METER: Spinner = Spinner {
        frames: &["▱▱▱", "▰▱▱", "▰▰▱", "▰▰▰", "▰▰▱", "▰▱▱"],
        fps: 7,
    };
    pub const HAMBURGER: Spinner = Spinner {
        frames: &["☱", "☲", "☴", "☲"],
        fps: 10,
    };
    pub const ELLIPSIS: Spinner = Spinner {
        frames: &["", ".", "..", "..."],
        fps: 3,
    };
    pub const GLOBE: Spinner = Spinner {
        frames: &["🌍", "🌎", "🌏"],
        fps: 4,
    };
    pub const MOON: Spinner = Spinner {
        frames: &["🌑", "🌒", "🌓", "🌔", "🌕", "🌖", "🌗", "🌘"],
        fps: 8,
    };
    pub const MONKEY: Spinner = Spinner {
        frames: &["🙈", "🙉", "🙊"],
        fps: 3,
    };
    pub const ARC: Spinner = Spinner {
        frames: &["◜", "◠", "◝", "◞", "◡", "◟"],
        fps: 7,
    };
    pub const NERD: Spinner = Spinner {
        frames: &["󰇙", "󰇙", "󰇙"],
        fps: 10,
    };
    pub const NERDARC: Spinner = Spinner {
        frames: &["◜", " ", "◝", "◞", "◡", "◟", " "],
        fps: 8,
    };
    pub const MINIDOT: Spinner = Spinner {
        frames: &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"],
        fps: 12,
    };
    /// A Nerd Font honeycomb cell that fills and empties, 2.5 s a cycle: slow enough to read
    /// as "waiting" next to a fast spinner.
    pub const HEXFILL: Spinner = Spinner {
        frames: &[
            "\u{f0ac3}",
            "\u{f0ac4}",
            "\u{f0ac5}",
            "\u{f0ac6}",
            "\u{f0ac7}",
            "\u{f0ac8}",
            "\u{f0ac7}",
            "\u{f0ac6}",
            "\u{f0ac5}",
            "\u{f0ac4}",
        ],
        fps: 4,
    };

    pub fn from_name(name: &str) -> &'static Spinner {
        match name {
            "line" => &Self::LINE,
            "jump" => &Self::JUMP,
            "pulse" => &Self::PULSE,
            "points" => &Self::POINTS,
            "meter" => &Self::METER,
            "hamburger" => &Self::HAMBURGER,
            "ellipsis" => &Self::ELLIPSIS,
            "globe" => &Self::GLOBE,
            "moon" => &Self::MOON,
            "monkey" => &Self::MONKEY,
            "arc" => &Self::ARC,
            "nerd" => &Self::NERD,
            "nerdarc" => &Self::NERDARC,
            "minidot" => &Self::MINIDOT,
            "hexfill" => &Self::HEXFILL,
            _ => &Self::DOT, // default
        }
    }

    pub fn current_frame(&self) -> &'static str {
        let ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default() // clock before 1970: treat as 0
            .as_millis() as u64;
        let ms_per_frame = 1000 / (self.fps as u64);
        let current_frame = (ms / ms_per_frame) % (self.frames.len() as u64);
        self.frames[current_frame as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::Spinner;

    #[test]
    fn hexfill_fills_and_empties_slowly() {
        let s = Spinner::from_name("hexfill");
        assert_eq!(s.frames.len(), 10);
        assert_eq!(s.fps, 4);
        // Rises from the empty cell to the full one, then falls back without repeating either end.
        assert_eq!(s.frames[0], "\u{f0ac3}");
        assert_eq!(s.frames[5], "\u{f0ac8}");
        assert!((1..5).all(|k| s.frames[k] == s.frames[10 - k]));
    }
}
