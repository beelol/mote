use mote_protocol::{ApprovalDecision, ApprovalVerdict, HostId, PendingApproval};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Denied,
    Expired,
    Failed,
}

pub struct Card<'a> {
    pub host_id: &'a HostId,
    pub host_name: &'a str,
    pub approval: &'a PendingApproval,
    pub status: ApprovalStatus,
    pub online: bool,
    pub in_flight: bool,
    pub now_ms: i64,
}

fn card_label(ui: &mut egui::Ui, key: &'static str, text: String) {
    let response = ui.label(&text);
    #[cfg(target_os = "ios")]
    crate::native_accessibility::record(
        ui,
        ui.make_persistent_id(("approval-label", key)),
        crate::native_accessibility::Role::Label,
        &text,
        None,
        &response,
    );
    #[cfg(not(target_os = "ios"))]
    let _ = (key, response);
}

pub fn draw(ui: &mut egui::Ui, card: Card<'_>) -> Option<ApprovalDecision> {
    let approval = card.approval;
    let mut decision = None;
    ui.push_id(
        (card.host_id, &approval.thread, &approval.approval_id),
        |ui| {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                card_label(ui, "title", format!("Browser request from {}", card.host_name));
                let site = approval
                    .payload
                    .get("site")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("Unknown site");
                let effect = approval
                    .payload
                    .get("effect")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("browser action");
                card_label(ui, "effect", format!("{effect} on {site}"));
                card_label(ui, "expiry", format!("Expires at {}", approval.expires_at));
                let label = match card.status {
                    ApprovalStatus::Pending if card.now_ms >= approval.expires_at => {
                        "Expired locally. Waiting for host confirmation"
                    }
                    ApprovalStatus::Pending if card.in_flight => "Waiting for host decision",
                    ApprovalStatus::Pending if !card.online => "Pending. Host is offline",
                    ApprovalStatus::Pending => "Pending",
                    ApprovalStatus::Approved => "Approved",
                    ApprovalStatus::Denied => "Denied",
                    ApprovalStatus::Expired => "Expired",
                    ApprovalStatus::Failed => "Failed",
                };
                card_label(ui, "status", label.to_owned());
                if card.status == ApprovalStatus::Pending {
                    let enabled =
                        card.online && !card.in_flight && card.now_ms < approval.expires_at;
                    ui.add_enabled_ui(enabled, |ui| {
                        ui.horizontal(|ui| {
                            for (index, label, verdict) in [
                                (0, "Approve", ApprovalVerdict::Approve),
                                (1, "Deny", ApprovalVerdict::Deny),
                            ] {
                                let response =
                                    ui.add_sized([112.0, 48.0], egui::Button::new(label));
                                #[cfg(target_os = "ios")]
                                crate::native_accessibility::record(
                                    ui,
                                    response.id,
                                    crate::native_accessibility::Role::Button,
                                    label,
                                    None,
                                    &response,
                                );
                                #[cfg(any(test, feature = "test-observation"))]
                                crate::observation::control(ui, "approval", &[index], &response);
                                #[cfg(not(any(test, feature = "test-observation")))]
                                let _ = index;
                                if response.clicked() {
                                    decision = Some(ApprovalDecision {
                                        host_id: card.host_id.clone(),
                                        thread: approval.thread.clone(),
                                        approval_id: approval.approval_id.clone(),
                                        action_hash: approval.action_hash.clone(),
                                        expires_at: approval.expires_at,
                                        decision: verdict,
                                    });
                                }
                            }
                        });
                    });
                }
            });
        },
    );
    decision
}

#[cfg(test)]
mod tests {
    use super::*;
    use mote_protocol::{ApprovalVerdict, PendingApproval};

    struct ApprovalState {
        approval: PendingApproval,
        status: ApprovalStatus,
    }

    fn state(status: ApprovalStatus) -> ApprovalState {
        let approval: PendingApproval = serde_json::from_value(serde_json::json!({
            "approval_id":"opaque-browser-id", "thread":"main", "kind":"browser",
            "payload":{"site":"http://127.0.0.1:8123","effect":"submit"},
            "action_hash":"1111111111111111111111111111111111111111111111111111111111111111",
            "expires_at":10000
        }))
        .unwrap();
        ApprovalState { approval, status }
    }

    fn frame(
        ctx: &egui::Context,
        state: &ApprovalState,
        online: bool,
        in_flight: bool,
        now_ms: i64,
        events: Vec<egui::Event>,
    ) -> (egui::FullOutput, Option<ApprovalDecision>) {
        let host = HostId::from_public_key(&[19; 32]);
        let mut decision = None;
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(834.0, 1210.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    decision = draw(
                        ui,
                        Card {
                            host_id: &host,
                            host_name: "Home",
                            approval: &state.approval,
                            status: state.status,
                            online,
                            in_flight,
                            now_ms,
                        },
                    );
                });
            },
        );
        (output, decision)
    }

    fn click(pos: egui::Pos2) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            },
        ]
    }

    #[test]
    fn pending_card_has_two_large_controls_and_emits_exact_bound_decisions() {
        for (index, verdict) in [(0, ApprovalVerdict::Approve), (1, ApprovalVerdict::Deny)] {
            let ctx = egui::Context::default();
            let state = state(ApprovalStatus::Pending);
            frame(&ctx, &state, true, false, 1000, vec![]);
            let controls = crate::observation::take_layout().controls;
            assert_eq!(controls.len(), 2);
            for control in &controls {
                assert!(control.hit.width() >= 48.0 && control.hit.height() >= 48.0);
            }
            let (_, decision) = frame(
                &ctx,
                &state,
                true,
                false,
                1000,
                click(controls[index].hit.center()),
            );
            let decision = decision.unwrap();
            assert_eq!(decision.host_id, HostId::from_public_key(&[19; 32]));
            assert_eq!(decision.thread, "main");
            assert_eq!(decision.approval_id, state.approval.approval_id);
            assert_eq!(decision.action_hash, state.approval.action_hash);
            assert_eq!(decision.expires_at, 10000);
            assert_eq!(decision.decision, verdict);
            crate::observation::take_layout();
        }
    }

    #[test]
    fn accesskit_approval_action_targets_only_the_enabled_bound_card() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let state = state(ApprovalStatus::Pending);
        frame(&ctx, &state, true, false, 1000, vec![]);
        let controls = crate::observation::take_layout().controls;
        let request = egui::Event::AccessKitActionRequest(accesskit::ActionRequest {
            action: accesskit::Action::Click,
            target: controls[0].id.value().into(),
            data: None,
        });
        let (_, decision) = frame(&ctx, &state, true, false, 1000, vec![request.clone()]);
        assert_eq!(decision.unwrap().decision, ApprovalVerdict::Approve);
        crate::observation::take_layout();
        let (_, denied) = frame(&ctx, &state, false, false, 1000, vec![request]);
        assert!(denied.is_none());
        crate::observation::take_layout();
    }

    #[test]
    fn offline_in_flight_and_locally_expired_cards_cannot_emit_a_decision() {
        for (online, in_flight, now) in [
            (false, false, 1000),
            (true, true, 1000),
            (true, false, 10000),
        ] {
            let ctx = egui::Context::default();
            let state = state(ApprovalStatus::Pending);
            frame(&ctx, &state, online, in_flight, now, vec![]);
            let controls = crate::observation::take_layout().controls;
            assert_eq!(controls.len(), 2);
            for control in controls {
                assert!(frame(
                    &ctx,
                    &state,
                    online,
                    in_flight,
                    now,
                    click(control.hit.center())
                )
                .1
                .is_none());
                crate::observation::take_layout();
            }
        }
    }

    #[test]
    fn terminal_cards_show_status_and_have_no_decision_controls() {
        for (status, label) in [
            (ApprovalStatus::Approved, "Approved"),
            (ApprovalStatus::Denied, "Denied"),
            (ApprovalStatus::Expired, "Expired"),
            (ApprovalStatus::Failed, "Failed"),
        ] {
            let ctx = egui::Context::default();
            let (output, decision) = frame(&ctx, &state(status), true, false, 1000, vec![]);
            assert!(decision.is_none());
            assert!(crate::observation::take_layout().controls.is_empty());
            assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
                egui::Shape::Text(text) if text.galley.text()==label)));
        }
    }
}
