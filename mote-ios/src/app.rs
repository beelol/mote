use mote_client_core::{
    gallery::{Gallery, GalleryPage},
    surface_v1,
};

pub struct MoteApp {
    gallery: Gallery,
    composer: String,
    last_event: String,
    reduced_motion: bool,
    live: Option<crate::live::LiveRuntime>,
    #[cfg(all(target_os = "ios", target_abi = "sim"))]
    probe: Option<crate::probe::native::Probe>,
}

impl Default for MoteApp {
    fn default() -> Self {
        Self {
            gallery: Gallery::default(),
            composer: String::new(),
            last_event: "No interaction yet".into(),
            reduced_motion: false,
            live: None,
            #[cfg(all(target_os = "ios", target_abi = "sim"))]
            probe: None,
        }
    }
}

impl eframe::App for MoteApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let safe_area = crate::safe_area();
        #[cfg(all(target_os = "ios", target_abi = "sim"))]
        if let Some(probe) = self.probe.as_mut() {
            if let Some(page) = probe.begin(ctx) {
                self.gallery.select(page);
            }
        }
        self.draw(ctx, safe_area);
        #[cfg(all(target_os = "ios", target_abi = "sim"))]
        if let Some(probe) = self.probe.as_mut() {
            probe.after_draw(ctx, safe_area);
        }
    }
}

impl MoteApp {
    pub fn attach_live(&mut self, ctx: &egui::Context) {
        match crate::live::LiveRuntime::from_environment(ctx.clone()) {
            Ok(runtime) => self.live = runtime,
            Err(error) => self.last_event = format!("Host setup failed: {error}"),
        }
    }
    pub fn with_launch_options(options: crate::simulator::LaunchOptions) -> Self {
        let mut app = Self::default();
        if let Some(page) = options.page {
            app.gallery.select(page);
        }
        app.reduced_motion = options.reduced_motion;
        #[cfg(all(target_os = "ios", target_abi = "sim"))]
        if options.timing_probe {
            app.gallery.select(GalleryPage::Layout);
            app.probe = crate::probe::native::Probe::new(options.reduced_motion);
        }
        app
    }

    fn draw(&mut self, ctx: &egui::Context, safe_area: egui::Margin) {
        if let Some(live) = &mut self.live {
            live.drain();
            live.draw(ctx, safe_area, &mut self.composer);
            return;
        }
        mote_client_core::transition::set_reduced_motion(ctx, self.reduced_motion);
        let mut status = egui::Frame::side_top_panel(&ctx.style());
        status.inner_margin.top = status.inner_margin.top.saturating_add(safe_area.top);
        status.inner_margin.left = status.inner_margin.left.saturating_add(safe_area.left);
        status.inner_margin.right = status.inner_margin.right.saturating_add(safe_area.right);
        egui::TopBottomPanel::top("status")
            .frame(status)
            .show(ctx, |ui| {
                ui.heading("Mote");
                ui.label("Offline gallery");
                ui.horizontal_wrapped(|ui| {
                    for page in GalleryPage::ALL {
                        if ui
                            .add_sized(
                                [96.0, 48.0],
                                egui::Button::new(page.name())
                                    .selected(self.gallery.selected() == page)
                                    .fill(ui.visuals().panel_fill),
                            )
                            .clicked()
                        {
                            self.gallery.select(page);
                            self.last_event = format!("Selected {}", page.name());
                        }
                    }
                });
            });
        let mut composer_frame = egui::Frame::side_top_panel(&ctx.style());
        composer_frame.inner_margin.left = composer_frame
            .inner_margin
            .left
            .saturating_add(safe_area.left);
        composer_frame.inner_margin.right = composer_frame
            .inner_margin
            .right
            .saturating_add(safe_area.right);
        egui::TopBottomPanel::top("composer")
            .frame(composer_frame)
            .show(ctx, |ui| {
                ui.label(&self.last_event);
                ui.label("Preview note");
                let width = ui.available_width();
                let response = ui.add_sized(
                    [width, 48.0],
                    egui::TextEdit::singleline(&mut self.composer)
                        .id(egui::Id::new("preview-note"))
                        .hint_text("Enter a preview note"),
                );
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Preview note")
                });
                #[cfg(test)]
                mote_client_core::observation::control(ui, "composer", &[], &response);
            });
        let mut body = egui::Frame::central_panel(&ctx.style());
        body.inner_margin.bottom = body.inner_margin.bottom.saturating_add(safe_area.bottom);
        body.inner_margin.left = body.inner_margin.left.saturating_add(safe_area.left);
        body.inner_margin.right = body.inner_margin.right.saturating_add(safe_area.right);
        egui::CentralPanel::default().frame(body).show(ctx, |ui| {
            let mut events = Vec::new();
            surface_v1::draw(
                ui,
                self.gallery
                    .surface()
                    .matrix_v1()
                    .expect("validated gallery"),
                &mut events,
            );
            for event in events {
                self.last_event = match event {
                    surface_v1::Event::Action { event, .. } => format!("Action: {event}"),
                    surface_v1::Event::Change { field, value, .. } => {
                        format!("Changed {field}: {value}")
                    }
                };
                ctx.request_repaint();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_browser_card_sends_once_then_replaces_controls_with_terminal_status() {
        use mote_host_client::{ConnectionState, HostDirectory, PairedHost};
        use mote_protocol::{ClientFrame, Event, RequestId, VersionRange};
        let host = PairedHost::new([21; 32], "Home", None, VersionRange::current()).unwrap();
        let id = host.host_id().clone();
        let mut directory = HostDirectory::default();
        directory.register(host).unwrap();
        directory.select_host(&id).unwrap();
        directory
            .set_connection(&id, ConnectionState::Online)
            .unwrap();
        let approval = serde_json::json!({"approval_id":"live-browser-request","thread":"main",
            "kind":"browser","payload":{"site":"http://127.0.0.1:8123","effect":"submit"},
            "action_hash":"1111111111111111111111111111111111111111111111111111111111111111",
            "expires_at":4102444800000_i64});
        let created = Event {
            seq: 1,
            ts: 0,
            scope_id: "main".into(),
            kind: "approval.created".into(),
            payload: serde_json::json!({"host_id":id,"approval":approval,"status":"pending"}),
        };
        directory.apply_event(&id, &id, created).unwrap();
        let (persist, rx) = std::sync::mpsc::channel();
        let (commands, outgoing) = std::sync::mpsc::channel();
        let mut app = MoteApp::default();
        app.live = Some(crate::live::LiveRuntime {
            directory,
            rx,
            persist,
            commands: std::collections::BTreeMap::from([(id.clone(), commands)]),
            status: Default::default(),
            approval_requests: Default::default(),
        });
        let ctx = egui::Context::default();
        let size = egui::vec2(834.0, 1210.0);
        frame(&mut app, &ctx, size, vec![]);
        frame(&mut app, &ctx, size, vec![]);
        let controls = mote_client_core::observation::take_layout().controls;
        let approve = controls
            .iter()
            .find(|control| control.kind == "approval" && control.path == [0])
            .unwrap();
        let events = || {
            vec![
                egui::Event::PointerMoved(approve.hit.center()),
                egui::Event::PointerButton {
                    pos: approve.hit.center(),
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
                egui::Event::PointerButton {
                    pos: approve.hit.center(),
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                },
            ]
        };
        frame(&mut app, &ctx, size, events());
        let command = outgoing.try_recv().unwrap();
        assert_eq!(command.host_id, id);
        assert!(
            matches!(command.frame,ClientFrame::ApprovalDecision {decision,..}
            if decision.approval_id==RequestId::new("live-browser-request").unwrap()
                && decision.host_id==id && decision.thread=="main")
        );
        app.live
            .as_ref()
            .unwrap()
            .persist
            .send(mote_host_client::HostUpdate::Failed {
                host_id: id.clone(),
                error: mote_protocol::HostError::new(
                    mote_protocol::HostErrorCode::Internal,
                    "indeterminate transport failure",
                ),
            })
            .unwrap();
        app.live.as_mut().unwrap().drain();
        frame(&mut app, &ctx, size, events());
        assert!(outgoing.try_recv().is_err());
        for (sequence, pending) in [(1, true), (2, true), (3, false), (4, true)] {
            app.live
                .as_ref()
                .unwrap()
                .persist
                .send(mote_host_client::HostUpdate::Frame {
                    host_id: id.clone(),
                    frame: mote_protocol::HostFrame::Snapshot {
                        snapshot: mote_protocol::HostSnapshot {
                            host_id: id.clone(),
                            cursor: mote_protocol::Cursor::new(id.clone(), sequence).unwrap(),
                            threads: vec![],
                            surfaces: vec![],
                            pending_approvals: if pending {
                                vec![serde_json::from_value(approval.clone()).unwrap()]
                            } else {
                                vec![]
                            },
                            status: mote_protocol::HostStatus::Online,
                        },
                    },
                })
                .unwrap();
            app.live.as_mut().unwrap().drain();
            assert_eq!(app.live.as_ref().unwrap().approval_requests.len(), 1);
            frame(&mut app, &ctx, size, vec![]);
            frame(&mut app, &ctx, size, events());
            assert!(outgoing.try_recv().is_err());
        }
        app.live.as_ref().unwrap().persist.send(mote_host_client::HostUpdate::Frame {
            host_id:id.clone(),frame:mote_protocol::HostFrame::Event {host_id:id.clone(),event:Event {seq:5,ts:0,
                scope_id:"main".into(),kind:"approval.updated".into(),payload:serde_json::json!({"host_id":id,"approval":approval,"status":"approved"})}}
        }).unwrap();
        app.live.as_mut().unwrap().drain();
        assert!(app.live.as_ref().unwrap().approval_requests.is_empty());
        let output = frame(&mut app, &ctx, size, vec![]);
        assert!(!mote_client_core::observation::take_layout()
            .controls
            .iter()
            .any(|control| control.kind == "approval"));
        assert!(output.shapes.iter().any(
            |shape| matches!(&shape.shape,egui::Shape::Text(text) if text.galley.text()=="Approved")
        ));
    }

    #[test]
    fn old_browser_decisions_cannot_hide_pending_controls_or_the_committed_result() {
        use mote_host_client::{ConnectionState, HostDirectory, PairedHost};
        use mote_protocol::{Event, VersionRange};
        for size in [
            egui::vec2(834.0, 1210.0),
            egui::vec2(1210.0, 834.0),
            egui::vec2(1032.0, 1376.0),
            egui::vec2(1376.0, 1032.0),
        ] {
            let host = PairedHost::new([23; 32], "Home", None, VersionRange::current()).unwrap();
            let id = host.host_id().clone();
            let mut directory = HostDirectory::default();
            directory.register(host).unwrap();
            directory.select_host(&id).unwrap();
            directory
                .set_connection(&id, ConnectionState::Online)
                .unwrap();
            for index in 0..76 {
                let pending = index == 75;
                directory.apply_event(&id,&id,Event {seq:index+1,ts:0,scope_id:"main".into(),
                    kind:if pending {"approval.created"} else {"approval.updated"}.into(),
                    payload:serde_json::json!({"host_id":id,"status":if pending {"pending"} else {"denied"},
                        "approval":{"approval_id":format!("request-{index:03}"),"thread":"main","kind":"browser",
                            "payload":{"site":"http://127.0.0.1:8123","effect":"submit"},
                            "action_hash":"1111111111111111111111111111111111111111111111111111111111111111",
                            "expires_at":4102444800000_i64}})}).unwrap();
            }
            let matrix = serde_json::json!({"version":1,"root":{"type":"screen","children":[{"type":"text","value":"Visible final result"}]}});
            mote_ui::v1::validate(&serde_json::to_vec(&matrix).unwrap()).unwrap();
            directory
                .apply_event(
                    &id,
                    &id,
                    Event {
                        seq: 77,
                        ts: 0,
                        scope_id: "main".into(),
                        kind: "ui.surface".into(),
                        payload: serde_json::json!({"surface_id":"matrix","matrix_surface":matrix}),
                    },
                )
                .unwrap();
            let (persist, rx) = std::sync::mpsc::channel();
            let mut app = MoteApp::default();
            app.live = Some(crate::live::LiveRuntime {
                directory,
                rx,
                persist,
                commands: Default::default(),
                status: Default::default(),
                approval_requests: Default::default(),
            });
            let ctx = egui::Context::default();
            frame(&mut app, &ctx, size, vec![]);
            let output = frame(&mut app, &ctx, size, vec![]);
            let controls = mote_client_core::observation::take_layout().controls;
            let pending: Vec<_> = controls
                .iter()
                .filter(|control| control.kind == "approval")
                .collect();
            assert_eq!(pending.len(), 2);
            for control in pending {
                assert!(control.hit.width() >= 48.0 && control.hit.height() >= 48.0);
                assert!(
                    control.clip.contains_rect(control.hit),
                    "pending control clipped at {size:?}"
                );
            }
            assert!(output.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text)
                if text.galley.text()=="Visible final result" && text.pos.y+text.galley.size().y <=size.y)));
            assert_eq!(
                app.live
                    .as_ref()
                    .unwrap()
                    .directory
                    .state(&id)
                    .unwrap()
                    .approval_states()
                    .len(),
                76
            );
        }
    }

    #[test]
    fn live_shell_labels_selected_host_and_rejects_cross_host_updates() {
        use mote_host_client::{HostDirectory, HostUpdate, PairedHost};
        use mote_protocol::{Event, HostFrame, VersionRange};
        let first = PairedHost::new([1; 32], "Home", None, VersionRange::current()).unwrap();
        let second = PairedHost::new([2; 32], "Office", None, VersionRange::current()).unwrap();
        let first_id = first.host_id().clone();
        let second_id = second.host_id().clone();
        let mut directory = HostDirectory::default();
        directory.register(first).unwrap(); directory.register(second).unwrap();
        directory.select_host(&first_id).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let mut app = MoteApp::default();
        app.live = Some(crate::live::LiveRuntime { directory, rx, persist: tx.clone(), commands: Default::default(), status: Default::default(), approval_requests: Default::default() });
        tx.send(HostUpdate::Frame { host_id: first_id.clone(), frame: HostFrame::Event { host_id: second_id.clone(), event: Event { seq: 1, ts: 0, scope_id: "main".into(), kind: "turn.user".into(), payload: serde_json::json!({"text":"Must not appear"}) } } }).unwrap();
        app.live.as_mut().unwrap().drain();
        let ctx = egui::Context::default();
        frame(&mut app, &ctx, egui::vec2(834.0, 1210.0), vec![]);
        let output = frame(&mut app, &ctx, egui::vec2(834.0, 1210.0), vec![]);
        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text().contains("Home"))));
        assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "Must not appear")));
        assert_eq!(app.live.as_ref().unwrap().directory.state(&first_id).unwrap().connection(), &mote_host_client::ConnectionState::IdentityChanged);
        tx.send(HostUpdate::Failed { host_id: second_id.clone(), error: mote_protocol::HostError::new(mote_protocol::HostErrorCode::Internal, "Office-only refusal") }).unwrap();
        app.live.as_mut().unwrap().drain();
        let output = frame(&mut app, &ctx, egui::vec2(834.0, 1210.0), vec![]);
        assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text().contains("Office-only refusal"))));
        app.live.as_mut().unwrap().directory.select_host(&second_id).unwrap();
        let output = frame(&mut app, &ctx, egui::vec2(834.0, 1210.0), vec![]);
        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text().contains("Office-only refusal"))));
    }

    fn frame(
        app: &mut MoteApp,
        ctx: &egui::Context,
        size: egui::Vec2,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        mote_client_core::observation::take_layout();
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                events,
                ..Default::default()
            },
            |ctx| app.draw(ctx, egui::Margin::ZERO),
        )
    }

    #[test]
    fn selecting_pages_installs_touch_sized_controls_on_both_ipad_sizes() {
        for size in [
            egui::vec2(834.0, 1210.0),
            egui::vec2(1210.0, 834.0),
            egui::vec2(1032.0, 1376.0),
            egui::vec2(1376.0, 1032.0),
        ] {
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            let mut app = MoteApp::default();
            frame(&mut app, &ctx, size, vec![]);
            for page in GalleryPage::ALL {
                let output = frame(&mut app, &ctx, size, vec![]);
                let update = output.platform_output.accesskit_update.as_ref().unwrap();
                let layout = mote_client_core::observation::take_layout();
                let composer = layout
                    .controls
                    .iter()
                    .find(|control| control.kind == "composer")
                    .unwrap();
                assert!(
                    composer.hit.width() >= 48.0 && composer.hit.height() >= 48.0,
                    "composer hit region: {composer:?}"
                );
                let mut page_targets = 0;
                for (_, node) in &update.nodes {
                    if node.label().is_some_and(|label| {
                        GalleryPage::ALL.iter().any(|page| page.name() == label)
                    }) {
                        if let Some(bounds) = node.bounds() {
                            page_targets += 1;
                            assert!(
                                bounds.width() >= 48.0 && bounds.height() >= 48.0,
                                "page hit target: {bounds:?}"
                            );
                        }
                    }
                }
                assert_eq!(page_targets, 8, "all page targets must be measurable");
                let center = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == page.name() => {
                            Some(text.pos + text.galley.size() / 2.0)
                        }
                        _ => None,
                    })
                    .expect("page selector is visible");
                frame(
                    &mut app,
                    &ctx,
                    size,
                    vec![
                        egui::Event::PointerMoved(center),
                        egui::Event::PointerButton {
                            pos: center,
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            modifiers: Default::default(),
                        },
                    ],
                );
                mote_client_core::observation::take_matrix();
                frame(
                    &mut app,
                    &ctx,
                    size,
                    vec![egui::Event::PointerButton {
                        pos: center,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: Default::default(),
                    }],
                );
                assert_eq!(app.gallery.selected(), page);
                let controls = mote_client_core::observation::take_matrix();
                assert!(controls.iter().any(|node| node.kind == "screen"));
                for node in controls
                    .iter()
                    .filter(|node| matches!(node.kind, "toggle" | "slider" | "select" | "button"))
                {
                    assert!(
                        node.rect.width() >= 48.0 && node.rect.height() >= 48.0,
                        "{} {} {:?}",
                        page.name(),
                        node.kind,
                        node.rect
                    );
                }
            }
        }
    }

    #[test]
    fn composer_accepts_text_input_without_a_matrix_contract_change() {
        let ctx = egui::Context::default();
        let mut app = MoteApp::default();
        let size = egui::vec2(834.0, 1210.0);
        frame(&mut app, &ctx, size, vec![]);
        ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("preview-note")));
        frame(
            &mut app,
            &ctx,
            size,
            vec![egui::Event::Text("iPad preview".into())],
        );
        assert_eq!(app.composer, "iPad preview");
    }

    #[test]
    fn composer_clears_the_keyboard_in_portrait_and_landscape() {
        for size in [egui::vec2(834.0, 1210.0), egui::vec2(1210.0, 834.0)] {
            let ctx = egui::Context::default();
            let mut app = MoteApp::default();
            frame(&mut app, &ctx, size, vec![]);
            let output = frame(&mut app, &ctx, size, vec![]);
            let position = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == "Enter a preview note" => {
                        Some(text.pos.y)
                    }
                    _ => None,
                })
                .unwrap();
            assert!(
                position < 300.0,
                "composer is behind the software keyboard at {position}"
            );
        }
    }

    #[test]
    fn list_last_action_scrolls_into_the_visible_hit_region() {
        for size in [
            egui::vec2(834.0, 1210.0),
            egui::vec2(1210.0, 834.0),
            egui::vec2(1032.0, 1376.0),
            egui::vec2(1376.0, 1032.0),
        ] {
            let ctx = egui::Context::default();
            let mut app = MoteApp::with_launch_options(crate::simulator::LaunchOptions {
                page: Some(GalleryPage::List),
                ..Default::default()
            });
            frame(&mut app, &ctx, size, vec![]);
            frame(
                &mut app,
                &ctx,
                size,
                vec![
                    egui::Event::PointerMoved(egui::pos2(size.x * 0.85, size.y * 0.5)),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0.0, -5000.0),
                        modifiers: Default::default(),
                    },
                ],
            );
            for _ in 0..120 {
                frame(&mut app, &ctx, size, vec![]);
            }
            mote_client_core::observation::take_matrix();
            frame(&mut app, &ctx, size, vec![]);
            let last = mote_client_core::observation::take_matrix()
                .into_iter()
                .find(|node| node.kind == "button" && node.path == [0, 19, 1])
                .unwrap();
            let visible = last.rect.intersect(last.clip);
            assert!(
                visible.width() >= 48.0 && visible.height() >= 48.0,
                "last row is clipped at {size:?}: {last:?}"
            );
            let center = visible.center();
            frame(
                &mut app,
                &ctx,
                size,
                vec![egui::Event::PointerMoved(center)],
            );
            frame(
                &mut app,
                &ctx,
                size,
                vec![
                    egui::Event::PointerMoved(center),
                    egui::Event::PointerButton {
                        pos: center,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: Default::default(),
                    },
                ],
            );
            frame(
                &mut app,
                &ctx,
                size,
                vec![egui::Event::PointerButton {
                    pos: center,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                }],
            );
            assert_eq!(app.last_event, "Action: gallery.item.20");
        }
    }

    #[test]
    fn shell_exposes_all_eight_gallery_pages() {
        let ctx = egui::Context::default();
        let mut app = MoteApp::default();
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(834.0, 1210.0),
                )),
                ..Default::default()
            },
            |ctx| app.draw(ctx, egui::Margin::ZERO),
        );
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(834.0, 1210.0),
                )),
                ..Default::default()
            },
            |ctx| app.draw(ctx, egui::Margin::ZERO),
        );
        for name in [
            "Layout",
            "Toggle",
            "Slider",
            "Select",
            "List",
            "Form",
            "Dashboard",
            "Fixture",
        ] {
            assert!(
                output.shapes.iter().any(|shape| matches!(
                    &shape.shape, egui::Shape::Text(text) if text.galley.text() == name
                )),
                "missing page selector: {name}"
            );
        }
    }

    #[test]
    fn gallery_heading_clears_native_top_safe_area() {
        let ctx = egui::Context::default();
        let mut app = MoteApp::default();
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(834.0, 1210.0),
                )),
                ..Default::default()
            },
            |ctx| {
                app.draw(
                    ctx,
                    egui::Margin {
                        top: 32,
                        ..Default::default()
                    },
                )
            },
        );
        let heading = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "Mote" => Some(text),
                _ => None,
            })
            .expect("gallery heading is painted");
        assert!(
            heading.pos.y >= 32.0,
            "heading starts under the system status bar at {}",
            heading.pos.y
        );
    }
}
