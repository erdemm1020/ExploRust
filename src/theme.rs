use gpui::{Rgba, WindowAppearance, rgb};

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
    pub fn for_appearance(appearance: WindowAppearance) -> Self {
        match appearance {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Self::dark(),
            _ => Self::light(),
        }
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
