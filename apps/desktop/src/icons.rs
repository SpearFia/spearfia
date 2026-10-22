//! Line icons drawn with the painter on a 24×24 grid (so no icon font is needed). The
//! shapes follow the icons in the Figma file's Docs-style editor frame.

use crate::theme;
use egui::{Color32, Pos2, Rect, Response, Sense, Stroke, Ui, pos2, vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Undo,
    Redo,
    Search,
    Link,
    Comment,
    Clip,
    Bullets,
    Numbered,
    Checklist,
    Outdent,
    Indent,
    Clear,
    Doc,
    Plus,
    Down,
    Left,
    Close,
    Check,
    Pencil,
    Eye,
    Chat,
    Sfx,
    Music,
    Look,
    Transition,
    Table,
    AlignLeft,
    AlignCenter,
    AlignRight,
    Spacing,
    Highlight,
    Minus,
}

fn arc(c: Pos2, r: f32, from: f32, to: f32, n: usize) -> Vec<Pos2> {
    (0..=n)
        .map(|i| {
            let a = from + (to - from) * i as f32 / n as f32;
            pos2(c.x + r * a.cos(), c.y + r * a.sin())
        })
        .collect()
}

/// Paint `icon` into `rect` (square) in `color`.
pub fn paint(p: &egui::Painter, rect: Rect, icon: Icon, color: Color32) {
    let s = rect.width() / 24.0;
    let o = rect.min;
    let pt = |x: f32, y: f32| pos2(o.x + x * s, o.y + y * s);
    let st = Stroke::new((1.8 * s).max(1.2), color);
    let line = |pts: &[(f32, f32)]| {
        p.add(egui::Shape::line(
            pts.iter().map(|&(x, y)| pt(x, y)).collect(),
            st,
        ));
    };
    let closed = |pts: &[(f32, f32)]| {
        p.add(egui::Shape::closed_line(
            pts.iter().map(|&(x, y)| pt(x, y)).collect(),
            st,
        ));
    };
    let dot = |x: f32, y: f32| {
        p.circle_filled(pt(x, y), 1.5 * s, color);
    };
    match icon {
        Icon::Undo | Icon::Redo => {
            let flip = |x: f32| if icon == Icon::Redo { 24.0 - x } else { x };
            line(&[(flip(9.0), 14.0), (flip(4.0), 9.0), (flip(9.0), 4.0)]);
            let mut pts = vec![pt(flip(4.0), 9.0)];
            let c = pt(flip(14.0), 15.0);
            let (a0, a1) = if icon == Icon::Redo {
                (
                    -std::f32::consts::FRAC_PI_2,
                    -3.0 * std::f32::consts::FRAC_PI_2,
                )
            } else {
                (-std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_2)
            };
            pts.extend(arc(c, 6.0 * s, a0, a1, 12));
            pts.push(pt(flip(11.0), 21.0));
            p.add(egui::Shape::line(pts, st));
        }
        Icon::Search => {
            p.circle_stroke(pt(11.0, 11.0), 6.5 * s, st);
            line(&[(16.0, 16.0), (21.0, 21.0)]);
        }
        Icon::Link => {
            // Two links of a chain, at 45°.
            for (cx, cy) in [(9.0f32, 15.0f32), (15.0, 9.0)] {
                let (hw, hh) = (5.5f32, 2.8f32);
                let r = |x: f32, y: f32| {
                    let (c, sn) = (
                        std::f32::consts::FRAC_1_SQRT_2,
                        -std::f32::consts::FRAC_1_SQRT_2,
                    );
                    (cx + x * c - y * sn, cy + x * sn + y * c)
                };
                closed(&[r(-hw, -hh), r(hw, -hh), r(hw, hh), r(-hw, hh)]);
            }
        }
        Icon::Comment | Icon::Chat => {
            closed(&[
                (3.0, 4.0),
                (21.0, 4.0),
                (21.0, 16.0),
                (8.0, 16.0),
                (4.0, 20.0),
                (4.0, 16.0),
                (3.0, 16.0),
            ]);
            if icon == Icon::Comment {
                line(&[(12.0, 7.0), (12.0, 13.0)]);
                line(&[(9.0, 10.0), (15.0, 10.0)]);
            }
        }
        Icon::Clip => {
            closed(&[(2.0, 6.0), (16.0, 6.0), (16.0, 18.0), (2.0, 18.0)]);
            closed(&[(16.0, 12.0), (22.0, 8.0), (22.0, 16.0)]);
        }
        Icon::Bullets => {
            for y in [6.0, 12.0, 18.0] {
                dot(4.5, y);
                line(&[(9.0, y), (21.0, y)]);
            }
        }
        Icon::Numbered => {
            for (i, y) in [6.0f32, 12.0, 18.0].into_iter().enumerate() {
                line(&[(10.0, y), (21.0, y)]);
                p.text(
                    pt(4.5, y),
                    egui::Align2::CENTER_CENTER,
                    (i + 1).to_string(),
                    egui::FontId::monospace(7.5 * s),
                    color,
                );
            }
        }
        Icon::Checklist => {
            line(&[(3.0, 7.0), (5.0, 9.0), (9.0, 5.0)]);
            line(&[(3.0, 17.0), (5.0, 19.0), (9.0, 15.0)]);
            line(&[(13.0, 7.0), (21.0, 7.0)]);
            line(&[(13.0, 17.0), (21.0, 17.0)]);
        }
        Icon::Outdent | Icon::Indent => {
            line(&[(11.0, 6.0), (21.0, 6.0)]);
            line(&[(11.0, 12.0), (21.0, 12.0)]);
            line(&[(3.0, 18.0), (21.0, 18.0)]);
            if icon == Icon::Outdent {
                line(&[(7.0, 9.0), (4.0, 12.0), (7.0, 15.0)]);
            } else {
                line(&[(3.0, 9.0), (6.0, 12.0), (3.0, 15.0)]);
            }
        }
        Icon::Clear => {
            line(&[(5.0, 6.0), (19.0, 6.0)]);
            line(&[(12.0, 6.0), (12.0, 20.0)]);
            line(&[(4.0, 4.0), (20.0, 20.0)]);
        }
        Icon::Doc => {
            closed(&[
                (5.0, 2.0),
                (14.0, 2.0),
                (19.0, 7.0),
                (19.0, 22.0),
                (5.0, 22.0),
            ]);
            line(&[(14.0, 2.0), (14.0, 7.0), (19.0, 7.0)]);
            line(&[(8.5, 13.0), (15.5, 13.0)]);
            line(&[(8.5, 17.0), (15.5, 17.0)]);
        }
        Icon::Plus => {
            line(&[(12.0, 5.0), (12.0, 19.0)]);
            line(&[(5.0, 12.0), (19.0, 12.0)]);
        }
        Icon::Down => line(&[(6.0, 9.0), (12.0, 15.0), (18.0, 9.0)]),
        Icon::Left => line(&[(15.0, 6.0), (9.0, 12.0), (15.0, 18.0)]),
        Icon::Close => {
            line(&[(6.0, 6.0), (18.0, 18.0)]);
            line(&[(18.0, 6.0), (6.0, 18.0)]);
        }
        Icon::Check => line(&[(5.0, 12.5), (10.0, 17.5), (19.0, 7.0)]),
        Icon::Pencil => {
            closed(&[
                (16.0, 3.5),
                (20.5, 8.0),
                (8.0, 20.5),
                (3.0, 21.0),
                (3.5, 16.0),
            ]);
            line(&[(13.5, 6.0), (18.0, 10.5)]);
        }
        Icon::Eye => {
            let mut pts = arc(pt(12.0, 22.0), 13.0 * s, -2.4, -0.74, 10);
            pts.extend(arc(pt(12.0, 2.0), 13.0 * s, 0.74, 2.4, 10));
            p.add(egui::Shape::closed_line(pts, st));
            p.circle_stroke(pt(12.0, 12.0), 3.0 * s, st);
        }
        Icon::Sfx => {
            closed(&[
                (3.0, 9.5),
                (6.5, 9.5),
                (11.5, 5.5),
                (11.5, 18.5),
                (6.5, 14.5),
                (3.0, 14.5),
            ]);
            p.add(egui::Shape::line(
                arc(pt(13.0, 12.0), 4.0 * s, -0.9, 0.9, 8),
                st,
            ));
            p.add(egui::Shape::line(
                arc(pt(13.0, 12.0), 7.5 * s, -0.9, 0.9, 10),
                st,
            ));
        }
        Icon::Music => {
            line(&[(9.0, 18.0), (9.0, 5.0), (20.0, 3.0), (20.0, 16.0)]);
            p.circle_stroke(pt(6.5, 18.0), 2.6 * s, st);
            p.circle_stroke(pt(17.5, 16.0), 2.6 * s, st);
        }
        Icon::Look => {
            closed(&[(3.0, 5.0), (21.0, 5.0), (21.0, 19.0), (3.0, 19.0)]);
            p.circle_stroke(pt(8.5, 14.0), 2.2 * s, st);
        }
        Icon::Transition => {
            closed(&[(4.0, 6.0), (11.0, 12.0), (4.0, 18.0)]);
            closed(&[(20.0, 6.0), (13.0, 12.0), (20.0, 18.0)]);
        }
        Icon::AlignLeft => {
            line(&[(3.0, 6.0), (21.0, 6.0)]);
            line(&[(3.0, 12.0), (15.0, 12.0)]);
            line(&[(3.0, 18.0), (18.0, 18.0)]);
        }
        Icon::AlignCenter => {
            line(&[(3.0, 6.0), (21.0, 6.0)]);
            line(&[(7.0, 12.0), (17.0, 12.0)]);
            line(&[(5.0, 18.0), (19.0, 18.0)]);
        }
        Icon::AlignRight => {
            line(&[(3.0, 6.0), (21.0, 6.0)]);
            line(&[(9.0, 12.0), (21.0, 12.0)]);
            line(&[(6.0, 18.0), (21.0, 18.0)]);
        }
        Icon::Spacing => {
            line(&[(3.0, 7.0), (6.0, 4.0), (9.0, 7.0)]);
            line(&[(6.0, 4.0), (6.0, 20.0)]);
            line(&[(3.0, 17.0), (6.0, 20.0), (9.0, 17.0)]);
            line(&[(13.0, 6.0), (21.0, 6.0)]);
            line(&[(13.0, 12.0), (21.0, 12.0)]);
            line(&[(13.0, 18.0), (21.0, 18.0)]);
        }
        Icon::Highlight => {
            closed(&[
                (9.0, 11.0),
                (3.0, 17.0),
                (3.0, 20.0),
                (12.0, 20.0),
                (15.0, 17.0),
            ]);
            line(&[
                (22.0, 12.0),
                (17.4, 16.6),
                (8.0, 7.2),
                (14.0, 2.0),
                (22.0, 10.0),
            ]);
        }
        Icon::Minus => line(&[(5.0, 12.0), (19.0, 12.0)]),
        Icon::Table => {
            closed(&[(3.0, 4.0), (21.0, 4.0), (21.0, 20.0), (3.0, 20.0)]);
            line(&[(3.0, 10.0), (21.0, 10.0)]);
            line(&[(3.0, 15.0), (21.0, 15.0)]);
            line(&[(10.0, 4.0), (10.0, 20.0)]);
        }
    }
}

/// A square toolbar button with an icon. `active` tints it (e.g. bold is on).
pub fn button(ui: &mut Ui, icon: Icon, tip: &str, enabled: bool, active: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(
        vec2(28.0, 28.0),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let p = ui.painter();
    if active {
        p.rect_filled(rect, 5.0, theme::tint(theme::ACCENT, 0.18));
    } else if resp.hovered() && enabled {
        p.rect_filled(rect, 5.0, theme::BG_HOVER);
    }
    let color = if !enabled {
        theme::TEXT_MUTED
    } else if active {
        theme::ACCENT
    } else {
        theme::TEXT_2
    };
    paint(
        p,
        Rect::from_center_size(rect.center(), vec2(16.0, 16.0)),
        icon,
        color,
    );
    resp.on_hover_text(tip)
}

/// A toolbar button showing a letter in a document font (B, I, U, S).
pub fn letter(
    ui: &mut Ui,
    letter: &str,
    family: &str,
    tip: &str,
    enabled: bool,
    deco: Option<&str>,
) -> Response {
    let (rect, resp) = ui.allocate_exact_size(
        vec2(28.0, 28.0),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let p = ui.painter();
    if resp.hovered() && enabled {
        p.rect_filled(rect, 5.0, theme::BG_HOVER);
    }
    let color = if enabled {
        theme::TEXT
    } else {
        theme::TEXT_MUTED
    };
    let r = p.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        letter,
        egui::FontId::new(15.0, egui::FontFamily::Name(family.into())),
        color,
    );
    match deco {
        Some("under") => {
            p.line_segment(
                [pos2(r.left(), r.bottom()), pos2(r.right(), r.bottom())],
                Stroke::new(1.2, color),
            );
        }
        Some("strike") => {
            p.line_segment(
                [
                    pos2(r.left() - 1.0, r.center().y),
                    pos2(r.right() + 1.0, r.center().y),
                ],
                Stroke::new(1.2, color),
            );
        }
        _ => {}
    }
    resp.on_hover_text(tip)
}
