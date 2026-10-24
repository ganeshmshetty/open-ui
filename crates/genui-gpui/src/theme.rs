//! Design tokens and the renderer's only source of literal sizes and colors.
//!
//! Every value here comes from `spec/DESIGN.md` §§2–5 and §10. Components take
//! `UiTheme` values; no other renderer module may name a color or a pixel size
//! (SPEC §12.8 item 4, AGENTS.md quality bar).

use gpui_kit::base::{
    SemanticThemeTokens, Theme, ThemeAppearance,
    theme_tokens::{
        ColorTokens, RadiusTokens, ShadowTokens, SpacingTokens, TextStyleToken, TypographyTokens,
    },
};
use gpui_kit::{App, FontWeight, Hsla, Pixels, Rgba, SharedString, Size, black, px, rgb, size};

/// Light or dark appearance. Follows the active gpui-kit theme (DESIGN §10).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThemeMode {
    Light,
    Dark,
}

impl ThemeMode {
    pub fn is_dark(self) -> bool {
        matches!(self, Self::Dark)
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    fn appearance(self) -> ThemeAppearance {
        match self {
            Self::Light => ThemeAppearance::Light,
            Self::Dark => ThemeAppearance::Dark,
        }
    }
}

/// The five semantic status tones (DESIGN §4.2).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Tone {
    Neutral,
    Info,
    Success,
    Warning,
    Danger,
}

impl Tone {
    pub const ALL: [Tone; 5] = [
        Tone::Neutral,
        Tone::Info,
        Tone::Success,
        Tone::Warning,
        Tone::Danger,
    ];

    pub fn from_name(name: &str) -> Tone {
        match name {
            "info" => Tone::Info,
            "success" => Tone::Success,
            "warning" => Tone::Warning,
            "danger" => Tone::Danger,
            _ => Tone::Neutral,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Tone::Neutral => "neutral",
            Tone::Info => "info",
            Tone::Success => "success",
            Tone::Warning => "warning",
            Tone::Danger => "danger",
        }
    }

    pub fn index(self) -> usize {
        match self {
            Tone::Neutral => 0,
            Tone::Info => 1,
            Tone::Success => 2,
            Tone::Warning => 3,
            Tone::Danger => 4,
        }
    }
}

/// A tone's text, tinted fill and border (DESIGN §4.2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToneColors {
    pub fg: Hsla,
    pub bg: Hsla,
    pub border: Hsla,
}

/// DESIGN §6 rule 11: the maximum number of chart categories drawn.
pub const MAX_CHART_CATEGORIES: usize = 24;

/// The seven roles of the type scale (DESIGN §3).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextRole {
    Caption,
    Small,
    Body,
    BodyLg,
    Title,
    Heading,
    Mono,
}

/// Resolved size, line height and weight for one type role.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TypeStyle {
    pub size: Pixels,
    pub line_height: Pixels,
    pub weight: FontWeight,
}

impl TypeStyle {
    pub fn weight(self, weight: FontWeight) -> Self {
        Self { weight, ..self }
    }
}

/// Radius tokens (DESIGN §5).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Radius {
    None,
    Sm,
    Md,
    Lg,
    Xl,
    Full,
}

/// Control heights (DESIGN §5).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControlSize {
    Small,
    Medium,
    Large,
}

/// Our additions to gpui-base's semantic tokens (DESIGN §4.1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExtraColors {
    pub surface_subtle: Hsla,
    pub accent: Hsla,
    pub selection: Hsla,
}

/// Host overrides (DESIGN §10). Colors and fonts are replaceable; spacing and
/// type scales are not.
#[derive(Clone, Debug, Default)]
pub struct ThemeOverrides {
    pub background: Option<Hsla>,
    pub foreground: Option<Hsla>,
    pub surface_subtle: Option<Hsla>,
    pub accent: Option<Hsla>,
    pub border: Option<Hsla>,
    pub fonts: Option<(SharedString, SharedString)>,
    /// One multiplier from 0 to 2 for every radius (DESIGN §10).
    pub radius_scale: Option<f32>,
}

/// Resolved renderer theme. Cloneable and cheap; every field is a token.
#[derive(Clone, Debug)]
pub struct UiTheme {
    mode: ThemeMode,
    tokens: SemanticThemeTokens,
    extra: ExtraColors,
    tones: [ToneColors; 5],
    chart: [Hsla; 6],
    fonts: (SharedString, SharedString),
    radius_scale: f32,
}

impl UiTheme {
    /// Reads the active gpui-kit appearance and semantic theme (SPEC §10.5).
    pub fn from_app(cx: &App) -> Self {
        let base = Theme::global(cx);
        let mode = match base.appearance {
            ThemeAppearance::Dark => ThemeMode::Dark,
            ThemeAppearance::Light => ThemeMode::Light,
        };
        Self::for_mode(mode).with_tokens(base.tokens)
    }

    /// Deterministic theme for an explicit mode, used by the gallery and tests.
    pub fn for_mode(mode: ThemeMode) -> Self {
        let light = matches!(mode, ThemeMode::Light);
        let colors = if light {
            ColorTokens {
                background: color(0xffffff),
                foreground: color(0x0a0a0a),
                surface: color(0xffffff),
                surface_foreground: color(0x0a0a0a),
                primary: color(0x171717),
                primary_foreground: color(0xfafafa),
                secondary: color(0xf5f5f5),
                secondary_foreground: color(0x171717),
                muted: color(0xf5f5f5),
                muted_foreground: color(0x737373),
                accent: color(0x2563eb),
                accent_foreground: color(0xffffff),
                destructive: color(0xdc2626),
                destructive_foreground: color(0xffffff),
                border: color(0xe5e5e5),
                input: color(0xe5e5e5),
                ring: color(0x2563eb).alpha(0.5),
                selection: color(0x2563eb).alpha(0.25),
            }
        } else {
            ColorTokens {
                background: color(0x0a0a0a),
                foreground: color(0xfafafa),
                surface: color(0x0a0a0a),
                surface_foreground: color(0xfafafa),
                primary: color(0xfafafa),
                primary_foreground: color(0x171717),
                secondary: color(0x262626),
                secondary_foreground: color(0xfafafa),
                muted: color(0x1f1f1f),
                muted_foreground: color(0xa3a3a3),
                accent: color(0x60a5fa),
                accent_foreground: color(0x0a0a0a),
                destructive: color(0xdc2626),
                destructive_foreground: color(0xfafafa),
                border: color(0x262626),
                input: color(0x2f2f2f),
                ring: color(0x60a5fa).alpha(0.5),
                selection: color(0x60a5fa).alpha(0.3),
            }
        };
        let mut tokens = SemanticThemeTokens {
            colors,
            radius: RadiusTokens {
                none: px(0.),
                sm: px(4.),
                md: px(6.),
                lg: px(8.),
                xl: px(12.),
                full: px(9999.),
            },
            spacing: SpacingTokens {
                xxs: px(2.),
                xs: px(4.),
                sm: px(8.),
                md: px(12.),
                lg: px(16.),
                xl: px(24.),
                xxl: px(32.),
            },
            typography: TypographyTokens {
                sans: ".SystemUIFont".into(),
                mono: default_mono(),
                xs: type_token(12., 16.),
                sm: type_token(13., 18.),
                md: type_token(14., 20.),
                lg: type_token(16., 24.),
                xl: type_token(20., 28.),
                mono_md: type_token(13., 20.),
            },
            shadow: ShadowTokens::elevations(if light {
                Rgba::from(black()).alpha(0.12).into()
            } else {
                Rgba::from(black()).alpha(0.5).into()
            }),
        };
        // DESIGN's scales are authoritative over gpui-base's named defaults.
        tokens.spacing.xxs = px(2.);
        let extra = ExtraColors {
            surface_subtle: if light {
                color(0xfafafa)
            } else {
                color(0x141414)
            },
            accent: colors.accent,
            selection: colors.selection,
        };
        let tones = if light { light_tones() } else { dark_tones() };
        let chart = if light { light_chart() } else { dark_chart() };
        let fonts = (
            tokens.typography.sans.clone(),
            tokens.typography.mono.clone(),
        );
        Self {
            mode,
            tokens,
            extra,
            tones,
            chart,
            fonts,
            radius_scale: 1.0,
        }
    }

    pub fn with_tokens(mut self, tokens: SemanticThemeTokens) -> Self {
        self.extra.accent = tokens.colors.accent;
        self.extra.selection = tokens.colors.selection;
        self.fonts = (
            tokens.typography.sans.clone(),
            tokens.typography.mono.clone(),
        );
        self.tokens = tokens;
        self
    }

    /// Applies host overrides (DESIGN §10), clamping the radius multiplier.
    pub fn with_overrides(mut self, overrides: ThemeOverrides) -> Self {
        if let Some(value) = overrides.background {
            self.tokens.colors.background = value;
            self.tokens.colors.surface = value;
        }
        if let Some(value) = overrides.foreground {
            self.tokens.colors.foreground = value;
            self.tokens.colors.surface_foreground = value;
        }
        if let Some(value) = overrides.surface_subtle {
            self.extra.surface_subtle = value;
        }
        if let Some(value) = overrides.accent {
            self.extra.accent = value;
            self.tokens.colors.accent = value;
        }
        if let Some(value) = overrides.border {
            self.tokens.colors.border = value;
        }
        if let Some((sans, mono)) = overrides.fonts {
            self.fonts = (sans, mono);
        }
        if let Some(value) = overrides.radius_scale {
            self.radius_scale = value.clamp(0.0, 2.0);
        }
        self
    }

    /// Installs this theme as the global gpui-base and gpui-component theme.
    /// Hosts call this once after `gpui_kit::init` and after changing mode.
    pub fn install(&self, cx: &mut App) {
        let mut base = Theme::global(cx);
        base.appearance = self.mode.appearance();
        base.tokens = self.tokens.clone();
        cx.set_global(base);

        let mode = if self.mode.is_dark() {
            gpui_kit::component::ThemeMode::Dark
        } else {
            gpui_kit::component::ThemeMode::Light
        };
        gpui_kit::component::Theme::change(mode, None, cx);
        let component = gpui_kit::component::Theme::global_mut(cx);
        component.apply_semantic_tokens(&self.tokens);
        component.link = self.extra.accent;
        component.link_hover = self.extra.accent;
        component.link_active = self.extra.accent;
        component.mono_font_family = self.fonts.1.clone();
        // gpui-base's "accent" is a neutral hover color; DESIGN §4.1 maps it to
        // secondary. This also keeps TextView inline code neutral rather than
        // the renderer's blue interaction accent.
        component.accent = self.tokens.colors.secondary;
        component.accent_foreground = self.tokens.colors.foreground;
    }

    pub fn mode(&self) -> ThemeMode {
        self.mode
    }

    pub fn tokens(&self) -> &SemanticThemeTokens {
        &self.tokens
    }

    pub fn background(&self) -> Hsla {
        self.tokens.colors.background
    }

    pub fn foreground(&self) -> Hsla {
        self.tokens.colors.foreground
    }

    pub fn surface(&self) -> Hsla {
        self.tokens.colors.surface
    }

    pub fn surface_subtle(&self) -> Hsla {
        self.extra.surface_subtle
    }

    pub fn muted_foreground(&self) -> Hsla {
        self.tokens.colors.muted_foreground
    }

    pub fn border(&self) -> Hsla {
        self.tokens.colors.border
    }

    pub fn input_border(&self) -> Hsla {
        self.tokens.colors.input
    }

    pub fn accent(&self) -> Hsla {
        self.extra.accent
    }

    pub fn accent_foreground(&self) -> Hsla {
        self.tokens.colors.accent_foreground
    }

    pub fn ring(&self) -> Hsla {
        self.tokens.colors.ring
    }

    pub fn primary(&self) -> Hsla {
        self.tokens.colors.primary
    }

    pub fn primary_foreground(&self) -> Hsla {
        self.tokens.colors.primary_foreground
    }

    pub fn secondary(&self) -> Hsla {
        self.tokens.colors.secondary
    }

    pub fn selection(&self) -> Hsla {
        self.extra.selection
    }

    pub fn tone(&self, tone: Tone) -> ToneColors {
        self.tones[tone.index()]
    }

    pub fn chart_color(&self, series: usize) -> Hsla {
        self.chart[series % self.chart.len()]
    }

    /// DESIGN §2: a 4 px base, steps 0–8.
    pub fn space(&self, step: u8) -> Pixels {
        px(f32::from(step.min(8)) * 4.)
    }

    pub fn hairline(&self) -> Pixels {
        px(1.)
    }

    /// DESIGN §5: radii, scaled by the host's 0–2 multiplier.
    pub fn radius(&self, radius: Radius) -> Pixels {
        let base = match radius {
            Radius::None => 0.0,
            Radius::Sm => 4.0,
            Radius::Md => 6.0,
            Radius::Lg => 8.0,
            Radius::Xl => 12.0,
            Radius::Full => 9999.0,
        };
        px(base * self.radius_scale)
    }

    /// Markdown paragraph gap (DESIGN §7: 8 px between blocks).
    pub fn paragraph_gap(&self) -> gpui_kit::Rems {
        gpui_kit::rems(self.space(2).as_f32() / 16.0)
    }

    /// Markdown headings map down: `#` is design level 3 (16 px), `##` and
    /// deeper are level 4 (14 px) (DESIGN §7).
    pub fn markdown_heading(&self, level: u8) -> Pixels {
        if level <= 1 { px(16.) } else { px(14.) }
    }

    /// Tooltip maximum width; one overlay line holds a truncated value.
    pub fn tooltip_max_width(&self) -> Pixels {
        px(360.)
    }

    /// DESIGN §3 type roles.
    pub fn text(&self, role: TextRole) -> TypeStyle {
        match role {
            TextRole::Caption => TypeStyle {
                size: px(12.),
                line_height: px(16.),
                weight: FontWeight::NORMAL,
            },
            TextRole::Small => TypeStyle {
                size: px(13.),
                line_height: px(18.),
                weight: FontWeight::NORMAL,
            },
            TextRole::Body => TypeStyle {
                size: px(14.),
                line_height: px(20.),
                weight: FontWeight::NORMAL,
            },
            TextRole::BodyLg => TypeStyle {
                size: px(16.),
                line_height: px(24.),
                weight: FontWeight::NORMAL,
            },
            TextRole::Title => TypeStyle {
                size: px(20.),
                line_height: px(28.),
                weight: FontWeight::SEMIBOLD,
            },
            TextRole::Heading => TypeStyle {
                size: px(24.),
                line_height: px(32.),
                weight: FontWeight::SEMIBOLD,
            },
            TextRole::Mono => TypeStyle {
                size: px(13.),
                line_height: px(20.),
                weight: FontWeight::NORMAL,
            },
        }
    }

    /// Heading levels 1–4 (DESIGN §3).
    pub fn heading(&self, level: i64) -> TypeStyle {
        match level {
            1 => self.text(TextRole::Heading),
            2 => self.text(TextRole::Title),
            3 => TypeStyle {
                size: px(16.),
                line_height: px(24.),
                weight: FontWeight::SEMIBOLD,
            },
            _ => TypeStyle {
                size: px(14.),
                line_height: px(20.),
                weight: FontWeight::SEMIBOLD,
            },
        }
    }

    pub fn font_sans(&self) -> SharedString {
        self.fonts.0.clone()
    }

    pub fn font_mono(&self) -> SharedString {
        self.fonts.1.clone()
    }

    /// Tabular figures for numbers in tables, Stats and chart ticks (DESIGN §3).
    pub fn tabular_features(&self) -> gpui_kit::FontFeatures {
        gpui_kit::FontFeatures(std::sync::Arc::new(vec![("tnum".into(), 1)]))
    }

    /// Control heights (DESIGN §5).
    pub fn control_height(&self, size: ControlSize) -> Pixels {
        match size {
            ControlSize::Small => px(24.),
            ControlSize::Medium => px(32.),
            ControlSize::Large => px(40.),
        }
    }

    // ---- Layout rules (DESIGN §6) and component anatomy (DESIGN §7). ----

    /// Rule 1: program content's maximum width.
    pub fn message_max_width(&self) -> Pixels {
        px(760.)
    }

    /// Rule 3: extra space above a Heading that is not its container's first child.
    pub fn heading_extra_top(&self) -> Pixels {
        self.space(2)
    }

    /// Rule 5: every Grid cell is at least this wide.
    pub fn grid_min_cell(&self) -> Pixels {
        px(220.)
    }

    /// Rule 5: the Grid gap when the model does not choose one.
    pub fn grid_default_gap(&self) -> Pixels {
        self.space(3)
    }

    /// Rule 6: Card padding for the catalog default `padding: 4`.
    pub fn card_padding(&self) -> Pixels {
        self.space(4)
    }

    /// Rule 6: space between a Card's title row and its content.
    pub fn card_title_gap(&self) -> Pixels {
        self.space(3)
    }

    /// Rule 6: space between a Card's children.
    pub fn card_child_gap(&self) -> Pixels {
        self.space(2)
    }

    /// Rule 9: an Image's default height.
    pub fn image_default_height(&self) -> Pixels {
        px(160.)
    }

    /// Rule 9: the Image height range the builder clamps to.
    pub fn image_min_height(&self) -> Pixels {
        px(40.)
    }

    pub fn image_max_height(&self) -> Pixels {
        px(480.)
    }

    /// Rule 11: a chart's default plot height.
    pub fn chart_height(&self) -> Pixels {
        px(240.)
    }

    /// Rule 11: line width for line charts.
    pub fn chart_line_width(&self) -> Pixels {
        px(2.)
    }

    /// Rule 11: an SVG-free canvas paints gridlines with a hairline.
    pub fn chart_grid_width(&self) -> Pixels {
        px(1.)
    }

    /// Rule 11: category labels that do not fit are truncated.
    pub fn chart_label_max_chars(&self) -> usize {
        14
    }

    /// Rule 11: the width reserved for value-axis labels. Wider than `space(8)`
    /// so grouped numbers such as "14,000" stay on one line.
    pub fn chart_axis_width(&self) -> Pixels {
        px(72.)
    }

    /// The definite height of a chart's plot plus its category labels; the
    /// cached plot boundary uses it (SPEC §10.1).
    pub fn chart_block_height(&self) -> Pixels {
        px(self.chart_height().as_f32()
            + self.text(TextRole::Caption).line_height.as_f32()
            + self.space(1).as_f32())
    }

    /// Rule 11: at most this many categories are drawn.
    pub fn chart_max_categories(&self) -> usize {
        MAX_CHART_CATEGORIES
    }

    /// Rule 11: about four value gridlines.
    pub fn chart_tick_count(&self) -> usize {
        4
    }

    /// Divider margins (DESIGN §7).
    pub fn divider_margin(&self) -> Pixels {
        self.space(2)
    }

    /// Badge anatomy (DESIGN §7).
    pub fn badge_height(&self) -> Pixels {
        px(20.)
    }

    pub fn badge_padding_x(&self) -> Pixels {
        self.space(2)
    }

    pub fn badge_padding_y(&self) -> Pixels {
        px(2.)
    }

    /// Facts anatomy (DESIGN §7).
    pub fn fact_label_fraction(&self) -> f32 {
        0.4
    }

    pub fn fact_column_gap(&self) -> Pixels {
        self.space(6)
    }

    pub fn fact_row_gap(&self) -> Pixels {
        self.space(2)
    }

    /// List anatomy (DESIGN §7). Lists themselves land in M8; the token exists
    /// now so the placeholder and future renderer share one source.
    pub fn list_item_gap(&self) -> Pixels {
        self.space(2)
    }

    /// Overlay shadow (DESIGN §5). Overlays are kit popovers in phase 1.
    pub fn overlay_shadow(&self) -> Vec<gpui_kit::BoxShadow> {
        self.tokens.shadow.lg.clone()
    }

    /// The gallery/state-sheet window size. A demo constant, still a token so
    /// no other module contains a pixel literal.
    pub fn demo_window_size(&self) -> Size<Pixels> {
        size(px(940.), px(720.))
    }

    /// The state sheet's fixed component cell size, used for cached child views.
    pub fn state_cell_size(&self) -> Size<Pixels> {
        size(px(320.), px(220.))
    }

    /// A taller window for the component state sheet screenshots.
    pub fn state_sheet_window_size(&self) -> Size<Pixels> {
        size(px(940.), px(3400.))
    }

    /// Sidebar width in the gallery.
    pub fn sidebar_width(&self) -> Pixels {
        px(240.)
    }

    /// Inspector panel width in the debug views.
    pub fn inspector_width(&self) -> Pixels {
        px(380.)
    }

    /// Monospace line height for the inspector and code.
    pub fn inspector_line_height(&self) -> Pixels {
        px(18.)
    }
}

fn color(hex: u32) -> Hsla {
    Hsla::from(rgb(hex))
}

fn type_token(size: f32, line_height: f32) -> TextStyleToken {
    TextStyleToken {
        size: px(size),
        line_height: px(line_height),
        weight: FontWeight::NORMAL,
    }
}

fn default_mono() -> SharedString {
    if cfg!(target_os = "macos") {
        "Menlo".into()
    } else if cfg!(target_os = "windows") {
        "Consolas".into()
    } else {
        "DejaVu Sans Mono".into()
    }
}

fn light_tones() -> [ToneColors; 5] {
    [
        ToneColors {
            fg: color(0x404040),
            bg: color(0xf5f5f5),
            border: color(0xe5e5e5),
        },
        ToneColors {
            fg: color(0x1d4ed8),
            bg: color(0xeff6ff),
            border: color(0xbfdbfe),
        },
        ToneColors {
            fg: color(0x15803d),
            bg: color(0xf0fdf4),
            border: color(0xbbf7d0),
        },
        ToneColors {
            fg: color(0xb45309),
            bg: color(0xfffbeb),
            border: color(0xfde68a),
        },
        ToneColors {
            // DESIGN §4.2 originally listed #dc2626; that pair measures 4.41:1
            // on #fef2f2, below the same section's AA requirement. The slightly
            // darker #c81e1e measures 5.24:1. DESIGN.md is updated with this fix.
            fg: color(0xc81e1e),
            bg: color(0xfef2f2),
            border: color(0xfecaca),
        },
    ]
}

fn dark_tones() -> [ToneColors; 5] {
    [
        ToneColors {
            fg: color(0xd4d4d4),
            bg: color(0x1f1f1f),
            border: color(0x2e2e2e),
        },
        ToneColors {
            fg: color(0x93c5fd),
            bg: color(0x0f1d33),
            border: color(0x1e3a5f),
        },
        ToneColors {
            fg: color(0x86efac),
            bg: color(0x0d2416),
            border: color(0x1a4d2e),
        },
        ToneColors {
            fg: color(0xfcd34d),
            bg: color(0x2b1f08),
            border: color(0x5c4210),
        },
        ToneColors {
            fg: color(0xfca5a5),
            bg: color(0x2d0f0f),
            border: color(0x5c1d1d),
        },
    ]
}

fn light_chart() -> [Hsla; 6] {
    [
        color(0x0072b2),
        color(0xe69f00),
        color(0x009e73),
        color(0xcc79a7),
        color(0xd55e00),
        color(0x56b4e9),
    ]
}

fn dark_chart() -> [Hsla; 6] {
    [
        color(0x56b4e9),
        color(0xf0b84a),
        color(0x3cc79a),
        color(0xe09cc4),
        color(0xf08a4b),
        color(0x9ad0f0),
    ]
}

/// Test-only pixel constructor so layout tests keep all literals in the theme
/// module (§12.8 item 4 scans tests too).
#[cfg(test)]
pub fn test_px(value: f32) -> Pixels {
    px(value)
}

/// Relative luminance of an opaque color, for the WCAG AA tests below.
#[cfg(test)]
fn luminance(color: Hsla) -> f32 {
    let rgba = Rgba::from(color);
    let channel = |value: f32| {
        if value <= 0.039_28 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(rgba.r) + 0.7152 * channel(rgba.g) + 0.0722 * channel(rgba.b)
}

#[cfg(test)]
fn contrast(a: Hsla, b: Hsla) -> f32 {
    let (x, y) = (luminance(a), luminance(b));
    let (hi, lo) = if x > y { (x, y) } else { (y, x) };
    (hi + 0.05) / (lo + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn both_modes() -> [UiTheme; 2] {
        [
            UiTheme::for_mode(ThemeMode::Light),
            UiTheme::for_mode(ThemeMode::Dark),
        ]
    }

    #[test]
    fn every_tone_pair_meets_wcag_aa_in_both_modes() {
        for theme in both_modes() {
            for tone in Tone::ALL {
                let colors = theme.tone(tone);
                let on_bg = contrast(colors.fg, colors.bg);
                let on_canvas = contrast(colors.fg, theme.background());
                assert!(
                    on_bg >= 4.5,
                    "{} {} fg-on-bg {on_bg}",
                    theme.mode().name(),
                    tone.name()
                );
                assert!(
                    on_canvas >= 4.5,
                    "{} {} fg-on-canvas {on_canvas}",
                    theme.mode().name(),
                    tone.name()
                );
            }
        }
    }

    #[test]
    fn body_text_roles_meet_wcag_aa_on_canvas() {
        for theme in both_modes() {
            for (name, color) in [
                ("foreground", theme.foreground()),
                ("muted", theme.muted_foreground()),
                ("accent", theme.accent()),
                (
                    "secondary_foreground",
                    theme.tokens.colors.secondary_foreground,
                ),
            ] {
                let ratio = contrast(color, theme.background());
                assert!(
                    ratio >= 4.5,
                    "{} {name} on canvas {ratio}",
                    theme.mode().name()
                );
            }
            let primary = contrast(theme.primary_foreground(), theme.primary());
            assert!(
                primary >= 4.5,
                "{} primary foreground {primary}",
                theme.mode().name()
            );
        }
    }

    #[test]
    fn spacing_type_and_radius_scales_match_design() {
        let theme = UiTheme::for_mode(ThemeMode::Light);
        assert_eq!(theme.space(0), px(0.));
        assert_eq!(theme.space(1), px(4.));
        assert_eq!(theme.space(8), px(32.));
        assert_eq!(theme.space(9), px(32.));
        assert_eq!(theme.radius(Radius::Sm), px(4.));
        assert_eq!(theme.radius(Radius::Md), px(6.));
        assert_eq!(theme.radius(Radius::Lg), px(8.));
        assert_eq!(theme.radius(Radius::Xl), px(12.));
        assert_eq!(theme.text(TextRole::Caption).size, px(12.));
        assert_eq!(theme.text(TextRole::Small).size, px(13.));
        assert_eq!(theme.text(TextRole::Body).size, px(14.));
        assert_eq!(theme.text(TextRole::BodyLg).size, px(16.));
        assert_eq!(theme.text(TextRole::Title).size, px(20.));
        assert_eq!(theme.text(TextRole::Heading).size, px(24.));
        assert_eq!(theme.heading(3).size, px(16.));
        assert_eq!(theme.heading(4).size, px(14.));
        assert_eq!(theme.control_height(ControlSize::Medium), px(32.));
        assert_eq!(theme.message_max_width(), px(760.));
        assert_eq!(theme.chart_height(), px(240.));
    }

    #[test]
    fn host_radius_multiplier_is_clamped_and_colors_are_replaceable() {
        let theme = UiTheme::for_mode(ThemeMode::Light).with_overrides(ThemeOverrides {
            accent: Some(gpui_kit::hsla(0.6, 0.6, 0.4, 1.)),
            radius_scale: Some(2.5),
            ..Default::default()
        });
        assert_eq!(theme.radius(Radius::Md), px(12.));
        assert_eq!(theme.accent(), gpui_kit::hsla(0.6, 0.6, 0.4, 1.));
        let theme = theme.with_overrides(ThemeOverrides {
            radius_scale: Some(-1.0),
            ..Default::default()
        });
        assert_eq!(theme.radius(Radius::Md), px(0.));
    }

    #[test]
    fn chart_palettes_are_categorical_and_distinct() {
        for theme in both_modes() {
            for index in 0..theme.chart.len() {
                for other in index + 1..theme.chart.len() {
                    assert_ne!(
                        theme.chart_color(index),
                        theme.chart_color(other),
                        "{} chart {index} vs {other}",
                        theme.mode().name()
                    );
                }
            }
            assert_eq!(theme.chart_color(6), theme.chart_color(0));
        }
    }

    #[test]
    fn base_tokens_follow_design_scales() {
        let theme = UiTheme::for_mode(ThemeMode::Dark);
        assert_eq!(theme.tokens.radius.xl, px(12.));
        assert_eq!(theme.tokens.spacing.sm, px(8.));
        assert_eq!(theme.tokens.typography.xs.size, px(12.));
        assert_eq!(theme.tokens.typography.mono_md.size, px(13.));
        assert_eq!(theme.surface_subtle(), color(0x141414));
    }
}
