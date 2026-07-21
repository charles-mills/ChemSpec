//! Bundled application fonts.

use iced::{Font, font};

pub const INTER_REGULAR_BYTES: &[u8] = include_bytes!("../assets/fonts/inter/Inter-Regular.ttf");
pub const INTER_MEDIUM_BYTES: &[u8] = include_bytes!("../assets/fonts/inter/Inter-Medium.ttf");
pub const INTER_SEMIBOLD_BYTES: &[u8] = include_bytes!("../assets/fonts/inter/Inter-SemiBold.ttf");
pub const INTER_BOLD_BYTES: &[u8] = include_bytes!("../assets/fonts/inter/Inter-Bold.ttf");

pub const REGULAR: Font = Font::with_name("Inter");
pub const MEDIUM: Font = Font {
    weight: font::Weight::Medium,
    ..REGULAR
};
pub const SEMIBOLD: Font = Font {
    weight: font::Weight::Semibold,
    ..REGULAR
};
pub const BOLD: Font = Font {
    weight: font::Weight::Bold,
    ..REGULAR
};
