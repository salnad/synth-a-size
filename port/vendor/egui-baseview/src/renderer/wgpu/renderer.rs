use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use baseview::{WindowContext, dpi::PhysicalSize};
use egui::FullOutput;
use egui_wgpu::{
    RenderState, RendererOptions, ScreenDescriptor, WgpuError,
    wgpu::{
        Color, CommandEncoderDescriptor, Extent3d, RenderPassColorAttachment, RenderPassDescriptor,
        Surface, SurfaceConfiguration, TextureDescriptor, TextureDimension, TextureUsages,
        TextureView, TextureViewDescriptor,
    },
};

#[cfg(all(feature = "log", not(feature = "tracing")))]
use log::{error, info, warn};
#[cfg(feature = "tracing")]
use tracing::{error, info, warn};

pub use egui_wgpu::WgpuConfiguration;

/// synth-a-size patch: the format and sample count of the render pass that
/// egui paint callbacks draw into. Inserted into the egui-wgpu renderer's
/// `callback_resources` by [`create_render_state`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WgpuTarget {
    pub format: egui_wgpu::wgpu::TextureFormat,
    /// Always at least 1.
    pub msaa_samples: u32,
}

impl WgpuTarget {
    /// The target for `RendererOptions::msaa_samples` of `msaa_samples`.
    /// egui-wgpu reads 0 (its default) as no multisampling, and so does
    /// [`Renderer::render`]: the pass is single-sampled, and a pipeline built
    /// with a sample count of 0 fails validation.
    pub fn new(format: egui_wgpu::wgpu::TextureFormat, msaa_samples: u32) -> Self {
        Self {
            format,
            msaa_samples: msaa_samples.max(1),
        }
    }
}

/// synth-a-size patch: how many errors the device has reported outside any
/// error scope. wgpu's default handler panics on them, which inside a plugin
/// window takes the host down; [`create_render_state`] installs a handler
/// that logs and counts them instead, and inserts this counter into the
/// renderer's `callback_resources` so paint callbacks can stop drawing.
#[derive(Debug, Clone, Default)]
pub struct WgpuErrors(Arc<AtomicUsize>);

impl WgpuErrors {
    pub fn count(&self) -> usize {
        self.0.load(Ordering::Relaxed)
    }
}

const LOGGED_WGPU_ERRORS: usize = 8;

/// synth-a-size patch: creates the render state a window draws with, as
/// [`Renderer::new`] does: egui-wgpu's [`RenderState`] for `config`, a device
/// error handler that logs instead of panicking ([`WgpuErrors`]), and the
/// [`WgpuTarget`] for paint callbacks. Public so tests can build the same
/// state without a window (`surface` `None`).
pub async fn create_render_state(
    config: &GraphicsConfig,
    instance: &wgpu::Instance,
    surface: Option<&Surface<'static>>,
) -> Result<RenderState, WgpuError> {
    let state = RenderState::create(
        &config.wgpu_options,
        instance,
        surface,
        config.renderer_options,
    )
    .await?;

    let errors = WgpuErrors::default();
    let counter = errors.clone();
    state.device.on_uncaptured_error(Arc::new(move |e| {
        let seen = counter.0.fetch_add(1, Ordering::Relaxed);
        if seen < LOGGED_WGPU_ERRORS {
            error!("wgpu error (custom drawing is turned off for this window): {e}");
        } else if seen == LOGGED_WGPU_ERRORS {
            error!("more wgpu errors; no longer logging them");
        }
    }));

    let mut renderer = state.renderer.write();
    renderer.callback_resources.insert(WgpuTarget::new(
        state.target_format,
        config.renderer_options.msaa_samples,
    ));
    renderer.callback_resources.insert(errors);
    drop(renderer);
    Ok(state)
}

#[derive(Debug, Clone)]
pub struct GraphicsConfig {
    /// Controls whether to apply dithering to minimize banding artifacts.
    ///
    /// Dithering assumes an sRGB output and thus will apply noise to any input value that lies between
    /// two 8bit values after applying the sRGB OETF function, i.e. if it's not a whole 8bit value in "gamma space".
    /// This means that only inputs from texture interpolation and vertex colors should be affected in practice.
    ///
    /// Defaults to true.
    pub dithering: bool,

    /// Configures wgpu instance/device/adapter/surface creation and renderloop.
    pub wgpu_options: WgpuConfiguration,

    /// Additional options for the wgpu renderer.
    pub renderer_options: RendererOptions,
}

impl Default for GraphicsConfig {
    fn default() -> Self {
        Self {
            dithering: true,
            wgpu_options: Default::default(),
            renderer_options: Default::default(),
        }
    }
}

pub struct Renderer {
    render_state: Arc<RenderState>,
    instance: wgpu::Instance,
    surface: Surface<'static>,
    config: GraphicsConfig,
    msaa_texture_view: Option<TextureView>,
    msaa_samples: u32,
    width: u32,
    height: u32,
    // synth-a-size patch: whether a frame has been presented.
    presented: bool,
}

impl Renderer {
    pub fn new(window: WindowContext, config: GraphicsConfig) -> Result<Self, WgpuError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(
            Box::new(window.platform_handle()),
        ));

        let surface = instance.create_surface(window.platform_handle())?;

        // synth-a-size patch: the same count as the published WgpuTarget.
        let msaa_samples = config.renderer_options.msaa_samples.max(1);

        let state = Arc::new(pollster::block_on(create_render_state(
            &config,
            &instance,
            Some(&surface),
        ))?);

        Ok(Self {
            render_state: state,
            instance,
            surface,
            config,
            msaa_texture_view: None,
            msaa_samples,
            width: 0,
            height: 0,
            presented: false,
        })
    }

    pub fn max_texture_side(&self) -> usize {
        self.render_state
            .as_ref()
            .device
            .limits()
            .max_texture_dimension_2d as usize
    }

    fn configure_surface(&self, width: u32, height: u32) {
        let usage = TextureUsages::RENDER_ATTACHMENT;

        // synth-a-size patch: log instead of panicking.
        let Some(default_config) =
            self.surface
                .get_default_config(&self.render_state.adapter, width, height)
        else {
            error!("the window's surface is not supported by the adapter");
            return;
        };
        let mut surf_config = SurfaceConfiguration {
            usage,
            format: self.render_state.target_format,
            present_mode: self.config.wgpu_options.surface.present_mode,
            view_formats: vec![self.render_state.target_format],
            ..default_config
        };

        if let Some(desired_maximum_frame_latency) = self
            .config
            .wgpu_options
            .surface
            .desired_maximum_frame_latency
        {
            surf_config.desired_maximum_frame_latency = desired_maximum_frame_latency;
        }

        self.surface
            .configure(&self.render_state.device, &surf_config);
    }

    fn resize_and_generate_msaa_view(&mut self, width: u32, height: u32) {
        let render_state = self.render_state.as_ref();

        self.width = width;
        self.height = height;

        self.configure_surface(width, height);

        let texture_format = render_state.target_format;

        if self.msaa_samples > 1 {
            self.msaa_texture_view = Some(
                render_state
                    .device
                    .create_texture(&TextureDescriptor {
                        label: Some("egui_msaa_texture"),
                        size: Extent3d {
                            width,
                            height,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: self.msaa_samples.max(1),
                        dimension: TextureDimension::D2,
                        format: texture_format,
                        usage: TextureUsages::RENDER_ATTACHMENT,
                        view_formats: &[texture_format],
                    })
                    .create_view(&TextureViewDescriptor::default()),
            );
        }
    }

    pub fn render(
        &mut self,
        window: &baseview::WindowContext,
        clear_color: egui::Rgba,
        physical_size: PhysicalSize<u32>,
        pixels_per_point: f32,
        egui_ctx: &mut egui::Context,
        full_output: &mut FullOutput,
    ) {
        let shapes = std::mem::take(&mut full_output.shapes);

        let clipped_primitives = egui_ctx.tessellate(shapes, pixels_per_point);

        let mut encoder =
            self.render_state
                .device
                .create_command_encoder(&CommandEncoderDescriptor {
                    label: Some("encoder"),
                });

        let screen_descriptor = ScreenDescriptor {
            size_in_pixels: [physical_size.width, physical_size.height],
            pixels_per_point,
        };

        let user_cmd_bufs = {
            let mut renderer = self.render_state.renderer.write();
            for (id, image_deltas) in full_output.textures_delta.set.drain() {
                for image_delta in image_deltas {
                    renderer.update_texture(
                        &self.render_state.device,
                        &self.render_state.queue,
                        id,
                        &image_delta,
                    );
                }
            }

            renderer.update_buffers(
                &self.render_state.device,
                &self.render_state.queue,
                &mut encoder,
                &clipped_primitives,
                &screen_descriptor,
            )
        };

        if self.width != physical_size.width
            || self.height != physical_size.height
            || self.msaa_texture_view.is_none()
        {
            self.resize_and_generate_msaa_view(physical_size.width, physical_size.height);
        }

        let mut recreate_surface = false;
        let output_frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) => Some(texture),
            wgpu::CurrentSurfaceTexture::Occluded | wgpu::CurrentSurfaceTexture::Timeout => return,
            wgpu::CurrentSurfaceTexture::Suboptimal(_) | wgpu::CurrentSurfaceTexture::Outdated => {
                None
            }
            // synth-a-size patch: the device's error handler has logged it.
            wgpu::CurrentSurfaceTexture::Validation => {
                warn!("skipping a frame: the surface texture failed validation");
                return;
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                recreate_surface = true;
                None
            }
        };

        let Some(output_frame) = output_frame else {
            if recreate_surface {
                // synth-a-size patch: log instead of panicking.
                match self.instance.create_surface(window.platform_handle()) {
                    Ok(surface) => self.surface = surface,
                    Err(e) => {
                        error!("cannot recreate the window's surface: {e}");
                        return;
                    }
                }
            }

            self.configure_surface(self.width, self.height);
            return;
        };

        {
            let renderer = self.render_state.renderer.read();
            let frame_view = output_frame
                .texture
                .create_view(&TextureViewDescriptor::default());

            let (view, resolve_target) = if let Some(msaa_view) = &self.msaa_texture_view {
                (msaa_view, Some(&frame_view))
            } else {
                (&frame_view, None)
            };

            let render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("egui_render"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target,
                    ops: egui_wgpu::wgpu::Operations {
                        load: egui_wgpu::wgpu::LoadOp::Clear(Color {
                            r: clear_color[0] as f64,
                            g: clear_color[1] as f64,
                            b: clear_color[2] as f64,
                            a: clear_color[3] as f64,
                        }),
                        store: egui_wgpu::wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            // Forgetting the pass' lifetime means that we are no longer compile-time protected from
            // runtime errors caused by accessing the parent encoder before the render pass is dropped.
            // Since we don't pass it on to the renderer, we should be perfectly safe against this mistake here!
            renderer.render(
                &mut render_pass.forget_lifetime(),
                &clipped_primitives,
                &screen_descriptor,
            );
        }

        {
            let mut renderer = self.render_state.renderer.write();
            for id in &full_output.textures_delta.free {
                renderer.free_texture(id);
            }
        }

        let encoded = encoder.finish();

        self.render_state
            .queue
            .submit(user_cmd_bufs.into_iter().chain([encoded]));

        self.render_state.queue.present(output_frame);

        // synth-a-size patch: the editor-open smoke test looks for this line.
        if !self.presented {
            self.presented = true;
            let errors = self
                .render_state
                .renderer
                .read()
                .callback_resources
                .get::<WgpuErrors>()
                .map_or(0, WgpuErrors::count);
            info!(
                "egui-baseview: presented the first frame ({}x{} px, {:?}, {} sample(s), {} device \
                 error(s))",
                self.width, self.height, self.render_state.target_format, self.msaa_samples, errors
            );
        }
    }
}
