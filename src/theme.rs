use gpui::{Rgba, WindowAppearance, rgb};
use objc2_app_kit::{NSColor, NSColorSpace};

#[derive(Clone, Copy)]
pub struct Theme {
    pub bg: Rgba,
    pub sidebar: Rgba,
    pub border: Rgba,
    pub text: Rgba,
    pub dim: Rgba,
    pub hover: Rgba,
    pub accent: Rgba,
    pub on_accent: Rgba,
    pub folder: Rgba,
    pub file: Rgba,
}

impl Theme {
    /// Light or dark palette to match the system, with the accent color the
    /// user picked in System Settings.
    pub fn for_appearance(appearance: WindowAppearance) -> Self {
        let mut theme = match appearance {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Self::dark(),
            _ => Self::light(),
        };
        if let Some(accent) = system_accent() {
            theme.accent = accent;
        }
        theme
    }

    fn light() -> Self {
        Self {
            bg: rgb(0xffffff),
            sidebar: rgb(0xf3f3f5),
            border: rgb(0xe4e4e7),
            text: rgb(0x1d1d1f),
            dim: rgb(0x86868b),
            hover: rgb(0xececef),
            accent: rgb(0x0a68f0),
            on_accent: rgb(0xffffff),
            folder: rgb(0x4aa3f5),
            file: rgb(0xc7c7cc),
        }
    }

    fn dark() -> Self {
        Self {
            bg: rgb(0x1e1e20),
            sidebar: rgb(0x2a2a2d),
            border: rgb(0x3a3a3e),
            text: rgb(0xf2f2f4),
            dim: rgb(0x8e8e93),
            hover: rgb(0x333336),
            accent: rgb(0x0a68f0),
            on_accent: rgb(0xffffff),
            folder: rgb(0x3b95ec),
            file: rgb(0x636368),
        }
    }
}

/// The system accent color, as currently resolved for light or dark.
fn system_accent() -> Option<Rgba> {
    let color =
        NSColor::controlAccentColor().colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())?;
    Some(Rgba {
        r: color.redComponent() as f32,
        g: color.greenComponent() as f32,
        b: color.blueComponent() as f32,
        a: 1.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_system_accent_color() {
        let c = system_accent().expect("accent color");
        println!(
            "accent = {:.0} {:.0} {:.0}",
            c.r * 255.0,
            c.g * 255.0,
            c.b * 255.0
        );
        assert!(
            (0.0..=1.0).contains(&c.r) && (0.0..=1.0).contains(&c.g) && (0.0..=1.0).contains(&c.b)
        );
        assert!(c.b > c.r, "default accent is blue");
    }
}
