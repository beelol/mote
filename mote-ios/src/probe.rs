#[derive(Default)]
struct Timeline {
    frames: Vec<(u32, f64, f64)>,
    changed_at: Option<f64>,
    captures: usize,
    warmup_completed: bool,
}

#[derive(Default)]
struct LiveTimeline {
    baseline: Option<i64>,
    replacement: Option<(i64, f64)>,
}

#[derive(Clone, Debug, PartialEq)]
struct GeneratedSliderTarget {
    path: Vec<usize>,
    field: String,
    surface_value: f64,
    value: f64,
}

fn generated_router_budget_slider(surface: &mote_ui::v1::Surface) -> Option<GeneratedSliderTarget> {
    fn find(node: &mote_ui::v1::Node, path: &mut Vec<usize>) -> Option<GeneratedSliderTarget> {
        use mote_ui::v1::Node;
        match node {
            Node::Slider {
                field,
                value,
                max,
                step,
                ..
            } if field == "budgets.router" => {
                let increment = step.unwrap_or(1.0);
                Some(GeneratedSliderTarget {
                    path: path.clone(),
                    field: field.clone(),
                    surface_value: *value,
                    value: (value + increment).min(*max),
                })
            }
            Node::Screen { children, .. }
            | Node::Col { children, .. }
            | Node::Row { children, .. }
            | Node::Header { children }
            | Node::Footer { children } => {
                for (index, child) in children.iter().enumerate() {
                    path.push(index);
                    let found = find(child, path);
                    path.pop();
                    if found.is_some() {
                        return found;
                    }
                }
                None
            }
            _ => None,
        }
    }

    find(&surface.root, &mut Vec::new())
}

fn red_outline_pulses(shapes: &[egui::Shape]) -> usize {
    fn count(shape: &egui::Shape) -> usize {
        match shape {
            egui::Shape::Vec(shapes) => shapes.iter().map(count).sum(),
            egui::Shape::Path(path)
                if path.stroke.width == 2.0
                    && matches!(
                        &path.stroke.color,
                        egui::epaint::ColorMode::Solid(color)
                            if color.r() > color.g() && color.r() > color.b()
                    ) =>
            {
                1
            }
            _ => 0,
        }
    }

    shapes.iter().map(count).sum()
}

fn live_action_requested(args: impl IntoIterator<Item = String>) -> bool {
    args.into_iter()
        .any(|arg| arg == "--mote-live-transition-action")
}

impl LiveTimeline {
    fn observe(&mut self, revision: i64, time: f64) -> bool {
        let baseline = self.baseline.get_or_insert(revision);
        if *baseline != revision && self.replacement.is_none() {
            self.replacement = Some((revision, time));
            true
        } else {
            false
        }
    }
}

#[derive(Default, Debug)]
struct Step {
    sequence: u32,
    change: bool,
    capture: bool,
    warmup: bool,
}

impl Timeline {
    fn observe(&mut self, elapsed_ms: f64, input_time: f64) -> Option<Step> {
        let previous = self.frames.last().copied();
        if previous.is_some_and(|frame| frame.2 == input_time) {
            return None;
        }
        let sequence = self.frames.len() as u32 + 1;
        self.frames.push((sequence, elapsed_ms, input_time));
        let mut step = Step {
            sequence,
            warmup: sequence == 1,
            ..Default::default()
        };
        if self.captures == 0 && elapsed_ms >= 2000.0 && self.warmup_completed {
            step.capture = true;
            self.captures = 1;
        } else if self.changed_at.is_none() && self.captures == 1 {
            self.changed_at = Some(elapsed_ms);
            step.change = true;
        } else if let Some(changed_at) = self.changed_at {
            let delta = previous.map_or(0.0, |frame| elapsed_ms - frame.1);
            let target = match self.captures {
                1 => Some(60.0),
                2 => Some(120.0),
                _ => None,
            };
            if target.is_some_and(|target| elapsed_ms - changed_at >= target - delta) {
                step.capture = true;
                self.captures += 1;
            }
        }
        Some(step)
    }
}

fn decode_marker(image: &egui::ColorImage, origin: [usize; 2], stride: usize) -> Option<u32> {
    if stride == 0 || origin[1] >= image.size[1] {
        return None;
    }
    let mut sequence = 0;
    for bit in 0..32 {
        let x = origin[0].checked_add(bit * stride)?;
        if x >= image.size[0] {
            return None;
        }
        let color = image.pixels[origin[1] * image.size[0] + x];
        if color.g() > 200 && color.r() < 50 {
            sequence |= 1 << bit;
        } else if !(color.r() > 200 && color.g() < 50) {
            return None;
        }
    }
    Some(sequence)
}

fn cadence(timestamps: &[f64]) -> Option<(f64, f64, f64)> {
    let mut intervals: Vec<_> = timestamps
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .collect();
    if intervals.is_empty()
        || intervals
            .iter()
            .any(|delta| !delta.is_finite() || *delta <= 0.0)
    {
        return None;
    }
    let fps = intervals.len() as f64 * 1000.0 / (timestamps.last()? - timestamps.first()?);
    intervals.sort_by(f64::total_cmp);
    let p95 = intervals[(intervals.len() as f64 * 0.95).ceil() as usize - 1];
    Some((fps, p95, *intervals.last()?))
}

#[cfg(all(target_os = "ios", target_abi = "sim"))]
pub(crate) mod native {
    use super::*;

    use mote_client_core::gallery::GalleryPage;
    use std::{
        cell::RefCell,
        ffi::CStr,
        path::PathBuf,
        sync::{
            atomic::{AtomicI32, Ordering},
            Arc, Mutex,
        },
        time::{Instant, SystemTime, UNIX_EPOCH},
    };

    thread_local! {
        static LIVE: RefCell<Option<LiveProbe>> = const { RefCell::new(None) };
    }

    struct LiveProbe {
        timeline: LiveTimeline,
        frames: Vec<serde_json::Value>,
        directory: PathBuf,
        sequence: u32,
        inject_action: bool,
        saved: bool,
    }

    pub(crate) fn live_begin(ctx: &egui::Context) {
        if !std::env::args().any(|arg| arg == "--mote-live-transition-probe") {
            return;
        }
        let inject_action = live_action_requested(std::env::args());
        mote_client_core::transition::enable_materialization_observation(ctx);
        LIVE.with(|slot| {
            let mut slot = slot.borrow_mut();
            let probe = slot.get_or_insert_with(|| {
                extern "C" {
                    fn mote_ios_probe_documents() -> *const std::ffi::c_char;
                }
                let documents = unsafe {
                    CStr::from_ptr(mote_ios_probe_documents())
                        .to_str()
                        .expect("native Documents path")
                };
                let directory = PathBuf::from(documents).join("mote-live-transition");
                std::fs::create_dir_all(&directory).expect("probe directory");
                LiveProbe {
                    timeline: LiveTimeline::default(),
                    frames: Vec::new(),
                    directory,
                    sequence: 0,
                    inject_action,
                    saved: false,
                }
            });
            if probe.saved {
                return;
            }
            probe.sequence += 1;
            if probe.timeline.baseline.is_some() {
                let _ = std::fs::write(probe.directory.join("ready"), b"ready");
            }
            if probe
                .timeline
                .replacement
                .is_some_and(|(_, start)| ctx.input(|i| i.time) - start > 0.8)
            {
                let result = (|| -> anyhow::Result<()> {
                    let report = serde_json::json!({
                        "frames":probe.frames,
                        "replacement":probe.timeline.replacement,
                        "inject_action":probe.inject_action,
                        "present_timestamps":null,"scanout_timestamps":null,
                    });
                    std::fs::write(
                        probe.directory.join("report.json"),
                        serde_json::to_vec_pretty(&report)?,
                    )?;
                    Ok(())
                })();
                match result {
                    Ok(()) => probe.saved = true,
                    Err(error) => eprintln!("Live transition probe failed: {error}"),
                }
            }
            ctx.request_repaint();
        });
    }

    pub(crate) fn live_surface(
        ui: &egui::Ui,
        revision: i64,
        surface: &mote_ui::v1::Surface,
        replacement_candidate: bool,
    ) {
        LIVE.with(|slot| {
            let mut slot = slot.borrow_mut();
            let Some(probe) = slot.as_mut().filter(|probe| !probe.saved) else {
                return;
            };
            let time = ui.input(|input| input.time);
            let prime = probe.timeline.baseline.is_none();
            if !replacement_candidate {
                probe.timeline.baseline = Some(revision);
            }
            let first = probe.timeline.observe(revision, time);
            let slider = generated_router_budget_slider(surface);
            let target = slider.as_ref().map(|slider| {
                ui.id()
                    .with("matrix-contract-1")
                    .with(("slider", slider.path.clone()))
            });
            if first && probe.inject_action {
                let slider = slider.as_ref().expect("replacement router budget slider");
                let target = target.expect("replacement router budget target");
                ui.ctx().input_mut(|input| {
                    input.events.push(egui::Event::AccessKitActionRequest(
                        accesskit::ActionRequest {
                            action: accesskit::Action::SetValue,
                            target: target.value().into(),
                            data: Some(accesskit::ActionData::NumericValue(slider.value)),
                        },
                    ))
                });
            }
            if prime {
                let target = ui
                    .id()
                    .with("matrix-contract-1")
                    .with(("button", vec![0_usize, 0]));
                ui.ctx().input_mut(|input| {
                    input.events.push(egui::Event::AccessKitActionRequest(
                        accesskit::ActionRequest {
                            action: accesskit::Action::Click,
                            target: target.value().into(),
                            data: None,
                        },
                    ))
                });
            }
            probe.frames.push(serde_json::json!({
                "sequence":probe.sequence,"revision":revision,"time":time,
                "immediate_action_target":(first && probe.inject_action)
                    .then(|| target.expect("replacement target").value()),
                "slider_target":slider.as_ref().map(|slider| serde_json::json!({
                    "path":slider.path,"field":slider.field,
                    "surface_value":slider.surface_value,"value":slider.value,
                })),
                "paint_start":ui.ctx().graphics(|graphics| graphics.get(ui.layer_id()).map_or(0, |list| list.all_entries().count())),
            }));
        });
    }

    pub(crate) fn live_surface_drawn(
        ui: &egui::Ui,
        events: &[mote_client_core::surface_v1::Event],
        transition_namespace: egui::Id,
    ) {
        LIVE.with(|slot| {
            let mut slot = slot.borrow_mut();
            let Some(probe) = slot.as_mut().filter(|probe| !probe.saved) else {
                return;
            };
            let Some(frame) = probe.frames.last_mut() else {
                return;
            };
            if let Some(observation) = mote_client_core::transition::materialization_observation(
                ui.ctx(),
                transition_namespace,
                &[1],
                "button",
            ) {
                let mut observation =
                    serde_json::to_value(observation).expect("stroke observation");
                observation["path"] = serde_json::json!([1]);
                frame["changed_button"] = observation;
            }
            for (name, path, kind) in [
                ("header_rule", &[0_usize][..], "header-rule"),
                ("slider_rail", &[2_usize][..], "slider"),
                ("footer_rule", &[3_usize][..], "footer-rule"),
            ] {
                if let Some(observation) =
                    mote_client_core::transition::materialization_observation(
                        ui.ctx(),
                        transition_namespace,
                        path,
                        kind,
                    )
                {
                    let mut observation =
                        serde_json::to_value(observation).expect("stroke observation");
                    observation["path"] = serde_json::json!(path);
                    frame[name] = observation;
                }
            }
            frame["emitted_events"] = serde_json::json!(events
                .iter()
                .map(|event| format!("{event:?}"))
                .collect::<Vec<_>>());
            let start = frame["paint_start"].as_u64().unwrap() as usize;
            let shapes = ui.ctx().graphics(|graphics| {
                graphics
                    .get(ui.layer_id())
                    .map(|list| list.all_entries().skip(start).cloned().collect::<Vec<_>>())
                    .unwrap_or_default()
            });
            frame["rectangles"] = serde_json::json!(shapes
                .iter()
                .filter_map(|clipped| {
                    if let egui::Shape::Rect(rect) = &clipped.shape {
                        (rect.stroke.width == 1.0).then_some([
                            rect.rect.left(),
                            rect.rect.top(),
                            rect.rect.width(),
                            rect.rect.height(),
                        ])
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>());
            frame["line_segments"] = serde_json::json!(shapes
                .iter()
                .filter_map(|clipped| {
                    if let egui::Shape::LineSegment { points, stroke } = &clipped.shape {
                        (stroke.width > 0.0).then_some([
                            points[0].x,
                            points[0].y,
                            points[1].x,
                            points[1].y,
                        ])
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>());
            fn particles(shape: &egui::Shape) -> usize {
                match shape {
                    egui::Shape::Vec(shapes) => shapes.iter().map(particles).sum(),
                    egui::Shape::Rect(rect)
                        if rect.stroke.width == 0.0
                            && rect.rect.width() == 4.0
                            && rect.rect.height() == 4.0 =>
                    {
                        1
                    }
                    _ => 0,
                }
            }
            frame["particle_count"] = serde_json::json!(shapes
                .iter()
                .map(|clipped| particles(&clipped.shape))
                .sum::<usize>());
            frame["pulse_count"] = serde_json::json!(shapes
                .iter()
                .map(|clipped| red_outline_pulses(std::slice::from_ref(&clipped.shape)))
                .sum::<usize>());
        });
    }

    pub(crate) fn live_end(ctx: &egui::Context, safe_area: egui::Margin) {
        LIVE.with(|slot| {
            let mut slot = slot.borrow_mut();
            let Some(probe) = slot.as_mut().filter(|probe| !probe.saved) else {
                return;
            };
            let screen = ctx.input(|input| input.screen_rect());
            let point = egui::pos2(screen.right() - 112.0, safe_area.top as f32 + 8.0);
            let scale = ctx.pixels_per_point();
            if let Some(frame) = probe.frames.last_mut() {
                frame["marker_origin"] = serde_json::json!([
                    ((point.x + 1.5) * scale) as usize,
                    ((point.y + 2.0) * scale) as usize
                ]);
                frame["marker_stride"] = serde_json::json!((3.0 * scale) as usize);
            }
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new("live-transition-probe"),
            ));
            for bit in 0..32 {
                painter.rect_filled(
                    egui::Rect::from_min_size(
                        point + egui::vec2(bit as f32 * 3.0, 0.0),
                        egui::vec2(3.0, 4.0),
                    ),
                    0.0,
                    if probe.sequence & (1 << bit) == 0 {
                        egui::Color32::RED
                    } else {
                        egui::Color32::GREEN
                    },
                );
            }
        });
    }

    #[derive(Clone)]
    struct Request {
        sequence: u32,
        warmup: bool,
        origin: [usize; 2],
        stride: usize,
    }

    struct Capture {
        request: u32,
        warmup: bool,
        rendered: Option<u32>,
        received_ms: f64,
        image: Arc<egui::ColorImage>,
    }

    struct PaintObserver {
        origin: Instant,
        sequence: u32,
        frames: Arc<Mutex<Vec<(u32, f64)>>>,
    }

    impl egui_wgpu::CallbackTrait for PaintObserver {
        fn paint(
            &self,
            _info: egui::PaintCallbackInfo,
            _render_pass: &mut egui_wgpu::wgpu::RenderPass<'static>,
            _resources: &egui_wgpu::CallbackResources,
        ) {
            self.frames
                .lock()
                .unwrap()
                .push((self.sequence, self.origin.elapsed().as_secs_f64() * 1000.0));
        }
    }

    pub(crate) struct Probe {
        origin: Instant,
        timeline: Timeline,
        renders: Arc<Mutex<Vec<(u32, f64)>>>,
        captures: Vec<Capture>,
        directory: PathBuf,
        reduced_motion: bool,
        cycle: usize,
        sequence: u32,
        capture_pending: bool,
        warmup_pending: bool,
        saving: bool,
        saved: Arc<AtomicI32>,
    }

    impl Probe {
        pub(crate) fn new(reduced_motion: bool) -> Option<Self> {
            extern "C" {
                fn mote_ios_probe_documents() -> *const std::ffi::c_char;
            }
            // Foundation owns this stable app-container path. Copy it before retaining it.
            let documents = unsafe {
                let value = mote_ios_probe_documents();
                if value.is_null() {
                    return None;
                }
                CStr::from_ptr(value).to_str().ok()?.to_owned()
            };
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .ok()?
                .as_nanos();
            let directory = PathBuf::from(documents).join(format!("mote-timing-{stamp}"));
            if !directory.is_absolute() || std::fs::create_dir(&directory).is_err() {
                return None;
            }
            Some(Self {
                origin: Instant::now(),
                timeline: Timeline::default(),
                renders: Arc::default(),
                captures: Vec::new(),
                directory,
                reduced_motion,
                cycle: 0,
                sequence: 0,
                capture_pending: false,
                warmup_pending: false,
                saving: false,
                saved: Arc::default(),
            })
        }

        pub(crate) fn begin(&mut self, ctx: &egui::Context) -> Option<GalleryPage> {
            if self.saving {
                return None;
            }
            let elapsed_ms = self.origin.elapsed().as_secs_f64() * 1000.0;
            ctx.input(|input| {
                for event in &input.events {
                    if let egui::Event::Screenshot {
                        user_data, image, ..
                    } = event
                    {
                        if let Some(request) = user_data
                            .data
                            .as_ref()
                            .and_then(|data| data.downcast_ref::<Request>())
                        {
                            if self
                                .captures
                                .iter()
                                .any(|capture| capture.request == request.sequence)
                            {
                                continue;
                            }
                            let rendered = decode_marker(image, request.origin, request.stride);
                            if request.warmup {
                                self.timeline.warmup_completed = rendered.is_some();
                            }
                            self.captures.push(Capture {
                                request: request.sequence,
                                warmup: request.warmup,
                                rendered,
                                received_ms: elapsed_ms,
                                image: image.clone(),
                            });
                        }
                    }
                }
            });
            let step = self
                .timeline
                .observe(elapsed_ms, ctx.input(|input| input.time))?;
            self.sequence = step.sequence;
            self.capture_pending = step.capture;
            self.warmup_pending = step.warmup;
            if elapsed_ms >= 14000.0
                && (self
                    .captures
                    .iter()
                    .filter(|capture| !capture.warmup)
                    .count()
                    == 3
                    || elapsed_ms >= 15000.0)
            {
                self.save(ctx);
                return None;
            }
            ctx.request_repaint();
            if step.change {
                return Some(GalleryPage::Form);
            }
            if elapsed_ms >= 3000.0 && self.timeline.changed_at.is_some() {
                let cycle = (elapsed_ms / 500.0) as usize;
                if cycle != self.cycle {
                    self.cycle = cycle;
                    return Some(if cycle % 2 == 0 {
                        GalleryPage::Layout
                    } else {
                        GalleryPage::Form
                    });
                }
            }
            None
        }

        pub(crate) fn after_draw(&mut self, ctx: &egui::Context, safe_area: egui::Margin) {
            let screen = ctx.input(|input| input.screen_rect());
            let point = egui::pos2(screen.right() - 112.0, safe_area.top as f32 + 8.0);
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new("timing-probe"),
            ));
            let sequence = self.sequence;
            for bit in 0..32 {
                let rect = egui::Rect::from_min_size(
                    point + egui::vec2(bit as f32 * 3.0, 0.0),
                    egui::vec2(3.0, 4.0),
                );
                painter.rect_filled(
                    rect,
                    0.0,
                    if sequence & (1 << bit) != 0 {
                        egui::Color32::GREEN
                    } else {
                        egui::Color32::RED
                    },
                );
            }
            if !self.saving {
                painter.add(egui_wgpu::Callback::new_paint_callback(
                    egui::Rect::from_min_size(point, egui::vec2(96.0, 4.0)),
                    PaintObserver {
                        origin: self.origin,
                        sequence,
                        frames: self.renders.clone(),
                    },
                ));
            }
            let label = match self.saved.load(Ordering::Acquire) {
                1 => "Timing probe complete",
                -1 => "Timing probe failed",
                _ if self.saving => "Timing probe saving",
                _ => "Timing probe running",
            };
            painter.text(
                point + egui::vec2(-150.0, 16.0),
                egui::Align2::LEFT_TOP,
                label,
                egui::FontId::proportional(18.0),
                egui::Color32::WHITE,
            );
            if (self.capture_pending || self.warmup_pending) && !self.saving {
                let scale = ctx.pixels_per_point();
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(
                    Request {
                        sequence,
                        warmup: self.warmup_pending,
                        origin: [
                            ((point.x + 1.5) * scale) as usize,
                            ((point.y + 2.0) * scale) as usize,
                        ],
                        stride: (3.0 * scale) as usize,
                    },
                )));
                self.capture_pending = false;
                self.warmup_pending = false;
            }
        }

        fn save(&mut self, ctx: &egui::Context) {
            self.saving = true;
            let frames = std::mem::take(&mut self.timeline.frames);
            let renders = self.renders.lock().unwrap().clone();
            let captures = std::mem::take(&mut self.captures);
            let changed_at = self.timeline.changed_at;
            let directory = self.directory.clone();
            let reduced_motion = self.reduced_motion;
            let saved = self.saved.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let result = (|| -> Result<(), Box<dyn std::error::Error>> {
                    let window = |time: &f64| *time >= 4000.0 && *time < 14000.0;
                    let update_times: Vec<_> =
                        frames.iter().map(|frame| frame.1).filter(window).collect();
                    let render_times: Vec<_> =
                        renders.iter().map(|frame| frame.1).filter(window).collect();
                    let mut images = Vec::new();
                    let mut warmup = None;
                    for capture in captures {
                        let name = if capture.warmup {
                            "warmup.png".to_owned()
                        } else {
                            format!("capture-{}.png", images.len())
                        };
                        let bytes: Vec<_> = capture
                            .image
                            .pixels
                            .iter()
                            .flat_map(|color| color.to_array())
                            .collect();
                        image::save_buffer_with_format(
                            directory.join(&name),
                            &bytes,
                            capture.image.size[0] as u32,
                            capture.image.size[1] as u32,
                            image::ColorType::Rgba8,
                            image::ImageFormat::Png,
                        )?;
                        let rendered_at = capture.rendered.and_then(|sequence| {
                            frames
                                .iter()
                                .find(|frame| frame.0 == sequence)
                                .map(|frame| frame.1)
                        });
                        let metadata = serde_json::json!({
                            "file": name, "request_sequence": capture.request,
                            "decoded_render_sequence": capture.rendered,
                            "gpu_event_received_ms": capture.received_ms,
                            "render_update_ms": rendered_at,
                            "transition_elapsed_ms": rendered_at.zip(changed_at).map(|(frame, start)| frame - start),
                        });
                        if capture.warmup {
                            warmup = Some(metadata);
                        } else {
                            images.push(metadata);
                        }
                    }
                    let report = serde_json::json!({
                        "version": 1, "reduced_motion": reduced_motion,
                        "measurement_window_ms": [4000, 14000],
                        "app_updates": frames, "render_pass_encodings": renders,
                        "app_update_cadence_fps_p95_ms_max_ms": cadence(&update_times),
                        "render_encoding_cadence_fps_p95_ms_max_ms": cadence(&render_times),
                        "transition_changed_at_ms": changed_at, "captures": images,
                        "warmup_capture": warmup,
                        "present_timestamps": null, "scanout_timestamps": null,
                    });
                    std::fs::write(
                        directory.join("report.json"),
                        serde_json::to_vec_pretty(&report)?,
                    )?;
                    Ok(())
                })();
                match result {
                    Ok(()) => {
                        eprintln!("Mote timing probe saved {}", directory.display());
                        saved.store(1, Ordering::Release);
                    }
                    Err(error) => {
                        eprintln!("Mote timing probe failed: {error}");
                        saved.store(-1, Ordering::Release);
                    }
                }
                ctx.request_repaint();
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_router_budget_slider_is_the_immediate_replacement_target() {
        let surface = mote_ui::v1::validate(
            &serde_json::to_vec(&serde_json::json!({
                "version": 1,
                "root": {"type": "screen", "children": [
                    {"type": "button", "label": "Before", "event": "refresh"},
                    {"type": "slider", "label": "Router budget", "field": "budgets.router",
                        "value": 150, "min": 50, "max": 1000, "step": 10}
                ]}
            }))
            .unwrap(),
        )
        .unwrap();

        let target = generated_router_budget_slider(&surface).unwrap();
        assert_eq!(target.path, vec![1]);
        assert_eq!(target.field, "budgets.router");
        assert_eq!(target.surface_value, 150.0);
        assert_eq!(target.value, 160.0);
    }

    #[test]
    fn pulse_counter_only_counts_two_point_red_paths() {
        use egui::{Color32, Pos2, Shape, Stroke};

        let pulse = Shape::line(
            vec![Pos2::ZERO, Pos2::new(10.0, 0.0)],
            Stroke::new(2.0, Color32::from_rgb(255, 0, 0)),
        );
        let wrong_width = Shape::line(
            vec![Pos2::ZERO, Pos2::new(10.0, 0.0)],
            Stroke::new(1.0, Color32::from_rgb(255, 0, 0)),
        );
        let wrong_color = Shape::line(
            vec![Pos2::ZERO, Pos2::new(10.0, 0.0)],
            Stroke::new(2.0, Color32::WHITE),
        );

        assert_eq!(red_outline_pulses(&[pulse, wrong_width, wrong_color]), 1);
    }

    #[test]
    fn live_transition_action_is_explicitly_opt_in() {
        assert!(!live_action_requested([
            "Mote".into(),
            "--mote-live-transition-probe".into(),
        ]));
        assert!(live_action_requested([
            "Mote".into(),
            "--mote-live-transition-probe".into(),
            "--mote-live-transition-action".into(),
        ]));
    }

    #[test]
    fn paired_capture_waits_for_a_replacement_and_tracks_its_first_frame() {
        let mut timeline = LiveTimeline::default();
        assert!(!timeline.observe(10, 0.0));
        assert!(!timeline.observe(10, 1.0));
        assert!(timeline.observe(20, 2.0));
        assert!(!timeline.observe(20, 2.016));
        assert_eq!(timeline.replacement, Some((20, 2.0)));
    }

    #[test]
    fn first_probe_frame_prewarms_capture_without_changing_layout() {
        let mut timeline = Timeline::default();
        let first = timeline.observe(0.0, 0.0).unwrap();
        assert!(first.warmup);
        assert!(!first.capture);
        assert!(!first.change);
        assert!(!timeline.observe(16.0, 0.016).unwrap().warmup);
        assert_eq!(timeline.changed_at, None);
    }

    #[test]
    fn transition_waits_for_gpu_warmup_completion() {
        let mut timeline = Timeline::default();
        timeline.observe(0.0, 0.0);
        assert!(!timeline.observe(2000.0, 2.0).unwrap().capture);
        assert_eq!(timeline.changed_at, None);
        timeline.warmup_completed = true;
        assert!(timeline.observe(2500.0, 2.5).unwrap().capture);
        assert!(timeline.observe(2516.0, 2.516).unwrap().change);
        assert_eq!(timeline.changed_at, Some(2516.0));
    }

    #[test]
    fn duplicate_input_passes_do_not_become_repaint_frames() {
        let mut timeline = Timeline::default();
        assert_eq!(timeline.observe(0.0, 10.0).unwrap().sequence, 1);
        assert!(timeline.observe(1.0, 10.0).is_none());
        assert_eq!(timeline.observe(16.0, 10.016).unwrap().sequence, 2);
        assert_eq!(timeline.frames, vec![(1, 0.0, 10.0), (2, 16.0, 10.016)]);
    }

    #[test]
    fn captures_are_prearmed_for_real_first_midpoint_and_final_frames() {
        let mut timeline = Timeline {
            warmup_completed: true,
            ..Default::default()
        };
        assert!(!timeline.observe(0.0, 0.0).unwrap().capture);
        let first = timeline.observe(2000.0, 2.0).unwrap();
        assert!(first.capture);
        assert!(!first.change);
        assert!(timeline.observe(2016.0, 2.016).unwrap().change);
        assert_eq!(timeline.changed_at, Some(2016.0));
        assert!(!timeline.observe(2032.0, 2.032).unwrap().capture);
        assert!(!timeline.observe(2048.0, 2.048).unwrap().capture);
        assert!(timeline.observe(2064.0, 2.064).unwrap().capture);
        assert!(!timeline.observe(2080.0, 2.080).unwrap().capture);
        assert!(!timeline.observe(2096.0, 2.096).unwrap().capture);
        assert!(!timeline.observe(2112.0, 2.112).unwrap().capture);
        assert!(timeline.observe(2128.0, 2.128).unwrap().capture);
        assert!(!timeline.observe(2144.0, 2.144).unwrap().capture);
    }

    #[test]
    fn gpu_marker_decodes_literal_pixels_and_rejects_missing_marker() {
        let mut image = egui::ColorImage::new([32, 1], egui::Color32::RED);
        image.pixels[0] = egui::Color32::GREEN;
        image.pixels[2] = egui::Color32::GREEN;
        assert_eq!(decode_marker(&image, [0, 0], 1), Some(5));
        assert_eq!(decode_marker(&image, [1, 0], 1), None);
        image.pixels[5] = egui::Color32::BLACK;
        assert_eq!(decode_marker(&image, [0, 0], 1), None);
    }

    #[test]
    fn cadence_uses_measured_intervals_not_nominal_refresh_rate() {
        let (fps, p95, maximum) = cadence(&[0.0, 16.0, 33.0, 50.0, 70.0]).unwrap();
        assert!((fps - 57.142857).abs() < 0.000001);
        assert_eq!(p95, 20.0);
        assert_eq!(maximum, 20.0);
        assert!(cadence(&[]).is_none());
        assert!(cadence(&[0.0, 0.0]).is_none());
    }
}
