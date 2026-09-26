//! Immutable ASCII atlas: no glyph uploads or texture replacement while drawing a frame.
//! Rasterized Liberation Sans (SIL OFL, assets/ui/LICENSE_LIBERATION), 48px source.
use macroquad::prelude::*;
use std::cell::RefCell;
thread_local! { static ATLAS: RefCell<Option<Texture2D>> = const { RefCell::new(None) }; }
const ADVANCE: [f32; 95] = [
    13.3438, 13.3438, 17.0469, 26.7031, 26.7031, 42.6875, 32.0156, 9.1719, 15.9844, 15.9844,
    18.6875, 28.0312, 13.3438, 15.9844, 13.3438, 13.3438, 26.7031, 26.7031, 26.7031, 26.7031,
    26.7031, 26.7031, 26.7031, 26.7031, 26.7031, 26.7031, 13.3438, 13.3438, 28.0312, 28.0312,
    28.0312, 26.7031, 48.7344, 32.0156, 32.0156, 34.6719, 34.6719, 32.0156, 29.3281, 37.3438,
    34.6719, 13.3438, 24.0000, 32.0156, 26.7031, 39.9844, 34.6719, 37.3438, 32.0156, 37.3438,
    34.6719, 32.0156, 29.3281, 34.6719, 32.0156, 45.3125, 32.0156, 32.0156, 29.3281, 13.3438,
    13.3438, 13.3438, 22.5312, 26.7031, 15.9844, 26.7031, 26.7031, 24.0000, 26.7031, 26.7031,
    13.3438, 26.7031, 26.7031, 10.6719, 10.6719, 24.0000, 10.6719, 39.9844, 26.7031, 26.7031,
    26.7031, 26.7031, 15.9844, 24.0000, 13.3438, 26.7031, 24.0000, 34.6719, 24.0000, 24.0000,
    24.0000, 16.0312, 12.4688, 16.0312, 28.0312,
];
/// Initialize during loading. Resizing/fullscreen never reallocates the atlas.
pub fn initialize() {
    ATLAS.with(|cell| {
        if cell.borrow().is_none() {
            let texture = Texture2D::from_file_with_format(
                include_bytes!("../../assets/ui/game-text.png"),
                Some(ImageFormat::Png),
            );
            texture.set_filter(FilterMode::Linear);
            *cell.borrow_mut() = Some(texture);
        }
    });
}
pub fn measure_text(text: &str, _font: Option<&Font>, size: u16, scale: f32) -> TextDimensions {
    let s = size as f32 * scale / 48.;
    TextDimensions {
        width: text.chars().map(|c| ADVANCE[index(c)] * s).sum(),
        height: 48. * s,
        offset_y: 38. * s,
    }
}
fn index(c: char) -> usize {
    if (' '..='~').contains(&c) {
        c as usize - 32
    } else {
        31
    }
}
/// Same baseline/size convention as Macroquad, with a stable prebuilt texture.
pub fn draw_text(text: &str, x: f32, y: f32, size: f32, color: Color) -> TextDimensions {
    initialize();
    let s = size / 48.;
    let mut cursor = x;
    ATLAS.with(|cell| {
        let atlas = cell.borrow();
        let atlas = atlas.as_ref().unwrap();
        for c in text.chars() {
            let i = index(c);
            draw_texture_ex(
                atlas,
                cursor - 4. * s,
                y - 52. * s,
                color,
                DrawTextureParams {
                    source: Some(Rect::new(
                        (i % 16) as f32 * 64.,
                        (i / 16) as f32 * 80.,
                        64.,
                        80.,
                    )),
                    dest_size: Some(vec2(64. * s, 80. * s)),
                    ..Default::default()
                },
            );
            cursor += ADVANCE[i] * s;
        }
    });
    TextDimensions {
        width: cursor - x,
        height: size,
        offset_y: size * 38. / 48.,
    }
}

/// A cached world-space sign, rendered with the normal alpha-blended mesh pipeline.
pub fn sign_mesh(
    text: &str,
    origin: Vec3,
    right: Vec3,
    up: Vec3,
    height: f32,
    color: Color,
) -> Mesh {
    initialize();
    ATLAS.with(|cell| {
        let atlas = cell.borrow();
        let mut mesh = Mesh {
            vertices: vec![],
            indices: vec![],
            texture: atlas.clone(),
        };
        let mut cursor = 0.;
        let s = height / 48.;
        for c in text.chars() {
            let i = index(c);
            let x = (i % 16) as f32 * 64. / 1024.;
            let y = (i / 16) as f32 * 80. / 512.;
            let base = mesh.vertices.len() as u16;
            for (a, b) in [(0., 0.), (1., 0.), (1., 1.), (0., 1.)] {
                let p = origin + right * (cursor + (a * 64. - 4.) * s) + up * ((52. - b * 80.) * s);
                mesh.vertices.push(Vertex::new2(
                    p,
                    vec2(x + a * 64. / 1024., y + b * 80. / 512.),
                    color,
                ));
            }
            mesh.indices
                .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
            cursor += ADVANCE[i] * s;
        }
        mesh
    })
}
