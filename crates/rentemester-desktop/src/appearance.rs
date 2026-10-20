//! Native projection of the web application's DESIGN.md tokens.
use crate::design_tokens::*;
use gpui_kit::{
    component::{Theme, ThemeMode},
    *,
};
use std::{
    borrow::Cow,
    sync::{Arc, OnceLock},
};

pub fn logo(cx: &App) -> Arc<RenderImage> {
    static LOGO: OnceLock<Arc<RenderImage>> = OnceLock::new();
    LOGO.get_or_init(|| {
        Image::from_bytes(
            ImageFormat::Png,
            include_bytes!("../../../assets/branding/rentemester-app-icon-v1.png").to_vec(),
        )
        .to_image_data(cx.svg_renderer())
        .expect("bundled Rentemester logo must decode")
    })
    .clone()
}

pub fn init(cx: &mut App) {
    let fonts: Vec<Cow<'static, [u8]>> = vec![
        Cow::Borrowed(include_bytes!(
            "../assets/fonts/ibm-plex-sans-latin-400-normal.ttf"
        )),
        Cow::Borrowed(include_bytes!(
            "../assets/fonts/ibm-plex-sans-latin-600-normal.ttf"
        )),
        Cow::Borrowed(include_bytes!(
            "../assets/fonts/ibm-plex-mono-latin-400-normal.ttf"
        )),
        Cow::Borrowed(include_bytes!(
            "../assets/fonts/source-serif-4-latin-600-normal.ttf"
        )),
    ];
    if let Err(error) = cx.text_system().add_fonts(fonts) {
        eprintln!("Rentemester fonts unavailable: {error}");
    }
    Theme::update(cx, |t| {
        t.mode = ThemeMode::Light;
        t.font_family = BODY_FONT.into();
        t.mono_font_family = MONO_FONT.into();
        t.font_size = px(16.);
        t.radius = px(4.);
        t.radius_lg = px(8.);
        t.shadow = false;
        let paper = rgb(PAPER).into();
        let raised = rgb(PAPER_RAISED).into();
        let ink = rgb(INK).into();
        let muted = rgb(INK_MUTED).into();
        let border = rgb(BORDER).into();
        let strong = rgb(BORDER_STRONG).into();
        let soft = rgb(ACCENT_SOFT).into();
        t.background = paper;
        t.foreground = ink;
        t.border = border;
        t.input = strong;
        t.caret = ink;
        t.accent = soft;
        t.accent_foreground = ink;
        t.primary = ink;
        t.primary_hover = muted;
        t.primary_active = ink;
        t.primary_foreground = paper;
        t.secondary = raised;
        t.secondary_hover = paper;
        t.secondary_active = soft;
        t.secondary_foreground = ink;
        t.button = raised;
        t.button_hover = paper;
        t.button_active = soft;
        t.button_foreground = ink;
        t.button_primary = ink;
        t.button_primary_hover = muted;
        t.button_primary_active = ink;
        t.button_primary_foreground = paper;
        t.button_secondary = raised;
        t.button_secondary_hover = paper;
        t.button_secondary_active = soft;
        t.button_secondary_foreground = ink;
        t.muted = paper;
        t.muted_foreground = muted;
        t.popover = raised;
        t.popover_foreground = ink;
        t.colors.list = raised;
        t.list_head = paper;
        t.list_hover = paper;
        t.list_active = soft;
        t.list_active_border = strong;
        t.list_even = paper;
        t.table = raised;
        t.table_head = paper;
        t.table_head_foreground = muted;
        t.table_even = paper;
        t.table_hover = paper;
        t.table_row_border = border;
        t.sidebar = raised;
        t.sidebar_foreground = ink;
        t.sidebar_border = border;
        t.selection = soft;
        t.ring = rgb(INFO).into();
        t.link = rgb(ACCENT).into();
        t.link_hover = rgb(DANGER).into();
        t.link_active = rgb(ACCENT).into();
        t.danger = rgb(DANGER).into();
        t.danger_foreground = paper;
        t.success = rgb(SUCCESS).into();
        t.warning = rgb(WARNING).into();
    });
}
