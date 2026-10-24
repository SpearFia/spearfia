//! Explorer's OLE drag loop does not send winit cursor motion on Windows.
//! Poll only during a file drag: https://github.com/emilk/egui/issues/7913

pub fn pointer(ctx: &egui::Context) -> Option<egui::Pos2> {
    #[cfg(windows)]
    {
        use mouse_position::mouse_position::Mouse;
        if let Mouse::Position { x, y } = Mouse::get_mouse_position()
            && let Some(inner) = ctx.input(|i| i.viewport().inner_rect)
        {
            return client_position(
                egui::pos2(x as f32, y as f32),
                inner,
                ctx.pixels_per_point(),
            );
        }
    }
    ctx.pointer_hover_pos()
}

#[cfg(any(windows, test))]
fn client_position(physical: egui::Pos2, inner: egui::Rect, scale: f32) -> Option<egui::Pos2> {
    if !physical.is_finite() || !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    let screen = physical / scale;
    inner
        .contains(screen)
        .then(|| (screen - inner.min).to_pos2())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cursor_matches_client_coordinates_at_different_dpi_and_zoom() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let inner =
                egui::Rect::from_min_size(egui::pos2(-200.0, 50.0), egui::vec2(800.0, 600.0));
            assert_eq!(
                client_position(egui::pos2(-120.0, 170.0) * scale, inner, scale),
                Some(egui::pos2(80.0, 120.0))
            );
            assert!(client_position(egui::pos2(900.0, 700.0) * scale, inner, scale).is_none());
        }
        assert!(client_position(egui::Pos2::ZERO, egui::Rect::EVERYTHING, 0.0).is_none());
    }
}
