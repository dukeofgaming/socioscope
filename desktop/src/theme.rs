use gpui::{Rgba, rgb};

#[derive(Clone, Copy)]
pub struct Theme {
    pub bg: Rgba,
    pub stage: Rgba,
    pub raised: Rgba,
    pub line: Rgba,
    pub text: Rgba,
    pub muted: Rgba,
    pub faint: Rgba,
    pub bone: Rgba,
    pub ink: Rgba,
    pub warn: Rgba,
    pub monitor: Rgba,
}

pub fn theme() -> Theme {
    Theme {
        bg: rgb(0x0E0F12),
        stage: rgb(0x13151A),
        raised: rgb(0x1C1F27),
        line: rgb(0x2A2D36),
        text: rgb(0xF3F0E8),
        muted: rgb(0xA4A7B0),
        faint: rgb(0x6E727C),
        bone: rgb(0xF3F0E8),
        ink: rgb(0x14161B),
        warn: rgb(0xE07A5F),
        monitor: rgb(0x000000),
    }
}

const SPEAKERS: [u32; 6] = [0x8FB4FF, 0xE2A36A, 0x7EC8B8, 0xE08B9A, 0xD4C07A, 0xB0A6E4];

pub fn speaker_color(name: &str) -> Rgba {
    if name.is_empty() {
        return rgb(0x8B909C);
    }
    let mut hash = 2166136261u32;
    for byte in name.bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(16777619);
    }
    rgb(SPEAKERS[hash as usize % SPEAKERS.len()])
}

pub fn with_alpha(color: Rgba, alpha: f32) -> Rgba {
    Rgba {
        color: color.color,
        alpha,
    }
}
