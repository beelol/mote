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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn gallery_sections_cover_every_surface_variant() {
        fn collect(node: &Node, found: &mut BTreeSet<&'static str>) {
            match node {
                Node::Text { .. } => {
                    found.insert("Text");
                }
                Node::Stack { children } => {
                    found.insert("Stack");
                    for child in children {
                        collect(child, found);
                    }
                }
                Node::Row { children } => {
                    found.insert("Row");
                    for child in children {
                        collect(child, found);
                    }
                }
                Node::List { children } => {
                    found.insert("List");
                    for child in children {
                        collect(child, found);
                    }
                }
                Node::Table { .. } => {
                    found.insert("Table");
                }
                Node::Field { .. } => {
                    found.insert("Field");
                }
                Node::Toggle { .. } => {
                    found.insert("Toggle");
                }
                Node::Select { .. } => {
                    found.insert("Select");
                }
                Node::Action { .. } => {
                    found.insert("Action");
                }
                Node::Progress { .. } => {
                    found.insert("Progress");
                }
                Node::Chat { children } => {
                    found.insert("Chat");
                    for child in children {
                        collect(child, found);
                    }
                }
            }
        }

        let gallery = crate::gallery::tree();
        let Node::Stack { children } = &gallery else {
            panic!("gallery must have one page");
        };
        let headings: BTreeSet<&str> = children
            .iter()
            .filter_map(|node| match node {
                Node::Text { value } => Some(value.as_str()),
                _ => None,
            })
            .collect();
        let expected = [
            "Text", "Stack", "Row", "List", "Table", "Field", "Toggle", "Select", "Action",
            "Progress", "Chat",
        ];
        for heading in expected {
            assert!(
                headings.contains(heading),
                "missing labeled {heading} section"
            );
        }
        let mut found = BTreeSet::new();
        collect(&gallery, &mut found);
        assert_eq!(found, expected.into_iter().collect());
        let encoded = serde_json::to_vec(&gallery).unwrap();
        assert_eq!(mote_ui::validate(&encoded).unwrap(), gallery);
    }

}
