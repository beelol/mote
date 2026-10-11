use mote_ui::Node;

fn text(value: &str) -> Node {
    Node::Text {
        value: value.into(),
    }
}

pub fn tree() -> Node {
    Node::Stack {
        children: vec![
            text("Surface gallery"),
            text("A local preview of every surface component. Controls can be edited here."),
            text("Text"),
            text("A plain text reply can sit beside any other component."),
            text("Stack"),
            Node::Stack {
                children: vec![text("First stacked item"), text("Second stacked item")],
            },
            text("Row"),
            Node::Row {
                children: vec![text("Left item"), text("Right item")],
            },
            text("List"),
            Node::List {
                children: vec![text("Draft"), text("Review"), text("Ready")],
            },
            text("Table"),
            Node::Table {
                columns: vec!["Stage".into(), "State".into()],
                rows: vec![
                    vec!["Talk".into(), "Ready".into()],
                    vec!["Think".into(), "Waiting".into()],
                ],
            },
            text("Field"),
            Node::Field {
                label: "Display name".into(),
                name: "gallery_display_name".into(),
                value: "Mote".into(),
            },
            text("Toggle"),
            Node::Toggle {
                label: "Enable preview".into(),
                name: "gallery_preview_enabled".into(),
                value: true,
            },
            text("Select"),
            Node::Select {
                label: "Priority".into(),
                name: "gallery_priority".into(),
                options: vec!["Low".into(), "Normal".into(), "High".into()],
                value: "Normal".into(),
            },
            text("Action"),
            Node::Action {
                label: "Preview action".into(),
                event: "gallery.preview".into(),
            },
            text("Progress"),
            Node::Progress { value: 0.64 },
            text("Chat"),
            Node::Chat {
                children: vec![
                    text("User: Show the latest status."),
                    text("Mote: Ready to help."),
                ],
            },
        ],
    }
}

pub fn preview() -> crate::contract::Surface {
    crate::contract::Surface::Gallery {
        legacy: tree(),
        matrix: matrix_tree(),
    }
}

pub fn matrix_tree() -> mote_ui::v1::Surface {
    mote_ui::v1::validate(br#"{"version":1,"root":{"type":"screen","title":"Matrix v1","children":[{"type":"header","children":[{"type":"text","value":"Matrix v1"}]},{"type":"col","children":[{"type":"slider","label":"Volume","field":"gallery_volume","value":4,"min":0,"max":10,"step":2},{"type":"toggle","label":"Enable Matrix preview","field":"gallery_matrix_enabled","value":true},{"type":"select","label":"Priority","field":"gallery_matrix_priority","options":["Low","Normal","High"],"value":"Normal"},{"type":"row","wrapping":"nowrap","children":[{"type":"text","value":"Remaining space","fill":true},{"type":"button","label":"Preview","event":"gallery.matrix"}]},{"type":"text","value":"Fill keeps the footer below this body.","fill":true}]},{"type":"footer","children":[{"type":"row","children":[{"type":"button","label":"Confirm action","event":"gallery.confirm","intent":"danger","confirm":true},{"type":"button","label":"Disabled","event":"gallery.disabled","disabled":true}]}]}]}}"#).expect("local Matrix gallery must validate")
}
