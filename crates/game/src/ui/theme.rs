//! Small themed widget layer on top of bevy_ui. Colors are chosen to read on
//! dark underwater and cosmic backgrounds; state is never conveyed by color
//! alone (hover also changes the border).

use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui_widgets::Button;

#[derive(Resource, Debug, Clone)]
pub struct Theme {
    pub accent: Color,
    pub text: Color,
    pub text_dim: Color,
    pub button: Color,
    pub button_hover: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            accent: Color::srgb(0.45, 1.0, 0.85),
            text: Color::srgb(0.88, 0.95, 0.95),
            text_dim: Color::srgb(0.55, 0.68, 0.7),
            button: Color::srgba(0.02, 0.12, 0.14, 0.75),
            button_hover: Color::srgba(0.06, 0.28, 0.3, 0.9),
        }
    }
}

impl Theme {
    pub fn font(&self, size: f32) -> TextFont {
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        }
    }
}

pub fn button_hover_style(
    theme: Res<Theme>,
    mut q: Query<
        (&Hovered, &mut BackgroundColor, &mut BorderColor),
        (Changed<Hovered>, With<Button>),
    >,
) {
    for (hovered, mut bg, mut border) in &mut q {
        if hovered.get() {
            *bg = theme.button_hover.into();
            *border = BorderColor::all(theme.accent);
        } else {
            *bg = theme.button.into();
            *border = BorderColor::all(theme.accent.with_alpha(0.6));
        }
    }
}
