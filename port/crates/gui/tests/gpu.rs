// SPDX-License-Identifier: GPL-3.0-or-later

//! The quad shaders' pixels, rendered through wgpu. Built with the
//! `gpu-tests` feature (CI's GUI reference job, on Mesa's lavapipe); without a
//! GPU adapter these tests fail.

use sas_gui::colour::Colour;
use sas_gui::paint::{PixelPainter, RectF};
use sas_gui::primitives::{publish_target, Quad, QuadBatch, QuadKind};

/// Renders `batch` on black, `width` × `height` pixels at display scale 1,
/// and returns the RGBA pixels row by row.
fn render(width: u32, height: u32, batch: impl Fn() -> QuadBatch + 'static) -> Vec<u8> {
    let state = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    publish_target(&state, 1);
    let mut harness = egui_kittest::Harness::builder()
        .with_size((width as f32, height as f32))
        .with_pixels_per_point(1.0)
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(
            state,
        ))
        .build_ui(move |ui| {
            let area = ui.ctx().content_rect();
            let painter = ui.painter_at(area);
            let pixels = PixelPainter::new(&painter, area, 1.0);
            let full = RectF::new(0.0, 0.0, width as f32, height as f32);
            pixels.fill_rect(full, Colour(0xff000000));
            batch().paint(&pixels, full);
        });
    harness.run_steps(2);
    let image = harness.render().expect("render");
    assert_eq!((image.width(), image.height()), (width, height));
    image.into_raw()
}

/// A horizontal bar 20 pixels long and 8 thick, as `BarRenderer` sizes one
/// at display scale 1: `size` is half the length along it and the full
/// thickness across.
#[test]
fn bars_fade_over_two_pixels_along_and_one_across() {
    let image = render(40, 30, || {
        let mut quad = Quad::new(
            QuadKind::Bar,
            RectF::new(10.0, 10.0, 20.0, 8.0),
            Colour(0xff00ff00),
        );
        quad.shader_values[0] = 10.0;
        quad.shader_values[1] = 8.0;
        let mut batch = QuadBatch::new();
        batch.push(quad);
        batch
    });
    let green = |x: usize, y: usize| image[4 * (40 * y + x) + 1] as i32;
    let near = |value: i32, expected: i32| (value - expected).abs() <= 2;
    // More than two pixels from the ends and one from the sides.
    assert_eq!(green(20, 14), 255);
    // The end pixels' centres are half a pixel in: 0.5 × 10 / 20 = 0.25.
    for (x, y) in [(10, 14), (29, 14)] {
        assert!(near(green(x, y), 64), "({x}, {y}): {}", green(x, y));
    }
    // The next pixel in: 1.5 × 10 / 20 = 0.75.
    assert!(near(green(11, 14), 191), "{}", green(11, 14));
    // The side pixels: 0.5 × 8 / 8 = 0.5.
    for (x, y) in [(20, 10), (20, 17)] {
        assert!(near(green(x, y), 128), "({x}, {y}): {}", green(x, y));
    }
    // Outside the quad nothing is drawn.
    assert_eq!(green(9, 14), 0);
    assert_eq!(green(20, 18), 0);
}
