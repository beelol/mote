pub use mote_client_core::surface::*;

#[cfg(test)]
mod tests {
    use mote_ui::Node;
    fn click_events(pos: egui::Pos2) -> Vec<egui::Event> {
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
    fn markup_strings_are_exact_inert_text_shapes_through_the_real_client() {
        use crate::app::observation::ObservationHarness;
        use serde_json::json;
        let literals = [
            "<b>hi</b>",
            "<script>alert('hi')</script>",
            "# **bold** _italic_ `code` [link](https://example.com) ![image](asset)",
        ];
        let node = Node::Stack {
            children: literals.iter().map(|value| Node::Text { value: (*value).into() }).collect(),
        };
        let mut harness = ObservationHarness::default();
        harness.event(&mote_eventlog::Event {
            seq: 1,
            ts: 0,
            scope_id: "main".into(),
            kind: "ui.surface".into(),
            payload: json!({"surface_id":"matrix", "surface":node}),
        });
        let (display, output) = harness.frame_output(vec![]);
        assert_eq!(harness.surface(), Some((1, node.clone())));
        assert_eq!(display.len(), 4);
        for (index, literal) in literals.iter().enumerate() {
            assert_eq!(display[index].path, [index]);
            assert_eq!(display[index].node, Node::Text { value: (*literal).into() });
            let text: Vec<_> = output.shapes.iter().filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if display[index].rect.contains(text.pos) => Some(text),
                _ => None,
            }).collect();
            assert_eq!(text.len(), 1, "extra or missing text shape for {literal}");
            assert_eq!(text[0].galley.text(), *literal);
            assert_eq!(text[0].galley.job.sections.len(), 1, "interpreted markup styling");
            assert!(output.platform_output.commands.is_empty());
            let (clicked, output) = harness.frame_output(click_events(display[index].rect.center()));
            assert_eq!(clicked, display);
            assert!(output.platform_output.commands.is_empty(), "link or clipboard command");
            assert!(harness.queued().is_empty());
            assert_eq!(harness.enter(), display);
            assert!(harness.queued().is_empty());
        }
        assert_eq!(display[3].node, node);
        assert!(display[3].path.is_empty());
        assert_eq!(harness.switches, 1);
    }

}
