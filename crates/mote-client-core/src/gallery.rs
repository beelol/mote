use mote_ui::Node;
use serde_json::json;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GalleryPage {
    Layout,
    Toggle,
    Slider,
    Select,
    List,
    Form,
    Dashboard,
    Fixture,
}

impl GalleryPage {
    pub const ALL: [Self; 8] = [
        Self::Layout,
        Self::Toggle,
        Self::Slider,
        Self::Select,
        Self::List,
        Self::Form,
        Self::Dashboard,
        Self::Fixture,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Layout => "Layout",
            Self::Toggle => "Toggle",
            Self::Slider => "Slider",
            Self::Select => "Select",
            Self::List => "List",
            Self::Form => "Form",
            Self::Dashboard => "Dashboard",
            Self::Fixture => "Fixture",
        }
    }

    fn surface(self) -> crate::contract::Surface {
        let matrix = match self {
            Self::Dashboard => mote_ui::v1::validate(include_bytes!(
                "../../../matrix/examples/mote-dashboard.v1.json"
            )),
            Self::Fixture => mote_ui::v1::validate(include_bytes!(
                "../../mote-ui/fixtures-v1/01_slider_layout.json"
            )),
            _ => {
                let children = match self {
                    Self::Layout => json!([
                        {"type":"header", "children":[{"type":"text", "value":"Anchored header"}]},
                        {"type":"col", "children":[
                            {"type":"row", "wrapping":"nowrap", "children":[
                                {"type":"text", "value":"Remaining width", "fill":true},
                                {"type":"button", "label":"Preview", "event":"gallery.layout"}
                            ]},
                            {"type":"text", "value":"The body fills the space between the header and footer.", "fill":true}
                        ]},
                        {"type":"footer", "children":[{"type":"text", "value":"Anchored footer"}]}
                    ]),
                    Self::Toggle => json!([
                        {"type":"toggle", "label":"Enable preview", "field":"gallery_enabled", "value":true}
                    ]),
                    Self::Slider => json!([
                        {"type":"slider", "label":"Volume", "field":"gallery_volume", "value":4, "min":0, "max":10, "step":2}
                    ]),
                    Self::Select => json!([
                        {"type":"select", "label":"Priority", "field":"gallery_priority", "options":["Low", "Normal", "High"], "value":"Normal"}
                    ]),
                    Self::List => json!([
                        {"type":"col", "children":[
                            {"type":"row", "children":[{"type":"text", "value":"Draft", "fill":true}, {"type":"button", "label":"Open draft", "event":"gallery.draft"}]},
                            {"type":"row", "children":[{"type":"text", "value":"Review", "fill":true}, {"type":"button", "label":"Open review", "event":"gallery.review"}]},
                            {"type":"row", "children":[{"type":"text", "value":"Ready", "fill":true}, {"type":"button", "label":"Open ready", "event":"gallery.ready"}]}
                        ]}
                    ]),
                    Self::Form => json!([
                        {"type":"col", "children":[
                            {"type":"toggle", "label":"Enable preview", "field":"gallery_form_enabled", "value":true},
                            {"type":"select", "label":"Priority", "field":"gallery_form_priority", "options":["Low", "Normal", "High"], "value":"Normal"},
                            {"type":"slider", "label":"Volume", "field":"gallery_form_volume", "value":4, "min":0, "max":10, "step":2},
                            {"type":"row", "children":[{"type":"button", "label":"Submit preview", "event":"gallery.submit", "intent":"primary"}]}
                        ]}
                    ]),
                    Self::Dashboard | Self::Fixture => unreachable!(),
                };
                let encoded = serde_json::to_vec(&json!({
                    "version":1,
                    "root":{"type":"screen", "title":self.name(), "children":children}
                }))
                .expect("local gallery data must serialize");
                mote_ui::v1::validate(&encoded)
            }
        }
        .expect("local Matrix gallery page must validate");
        crate::contract::Surface::V1 {
            surface: matrix,
            fallback: text("This Matrix surface requires a newer Mote client."),
        }
    }
}

pub struct Gallery {
    selected: GalleryPage,
    surface: crate::contract::Surface,
}

impl Default for Gallery {
    fn default() -> Self {
        let selected = GalleryPage::Layout;
        Self {
            selected,
            surface: selected.surface(),
        }
    }
}

impl Gallery {
    pub fn select(&mut self, page: GalleryPage) {
        self.surface = page.surface();
        self.selected = page;
    }

    pub fn selected(&self) -> GalleryPage {
        self.selected
    }

    pub fn surface(&self) -> &crate::contract::Surface {
        &self.surface
    }
}

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
    fn gallery_pages_cover_required_names_and_validate() {
        use serde_json::{json, Value};

        fn nodes<'a>(node: &'a Value, all: &mut Vec<&'a Value>) {
            all.push(node);
            if let Some(children) = node["children"].as_array() {
                for child in children {
                    nodes(child, all);
                }
            }
        }

        let cases = [
            (
                GalleryPage::Layout,
                "Layout",
                vec![
                    json!({"type":"header"}),
                    json!({"type":"footer"}),
                    json!({"type":"row", "wrapping":"nowrap"}),
                    json!({"type":"text", "fill":true}),
                ],
            ),
            (
                GalleryPage::Toggle,
                "Toggle",
                vec![json!({"type":"toggle", "field":"gallery_enabled", "value":true})],
            ),
            (
                GalleryPage::Slider,
                "Slider",
                vec![
                    json!({"type":"slider", "field":"gallery_volume", "value":4.0, "min":0.0, "max":10.0, "step":2.0}),
                ],
            ),
            (
                GalleryPage::Select,
                "Select",
                vec![
                    json!({"type":"select", "field":"gallery_priority", "options":["Low","Normal","High"], "value":"Normal"}),
                ],
            ),
            (
                GalleryPage::List,
                "List",
                vec![
                    json!({"type":"col"}),
                    json!({"type":"row"}),
                    json!({"type":"text", "value":"Draft"}),
                    json!({"type":"text", "value":"Review"}),
                    json!({"type":"text", "value":"Ready"}),
                ],
            ),
            (
                GalleryPage::Form,
                "Form",
                vec![
                    json!({"type":"toggle", "field":"gallery_form_enabled"}),
                    json!({"type":"select", "field":"gallery_form_priority"}),
                    json!({"type":"slider", "field":"gallery_form_volume"}),
                    json!({"type":"button", "event":"gallery.submit"}),
                ],
            ),
            (
                GalleryPage::Dashboard,
                "Dashboard",
                vec![
                    json!({"type":"toggle", "field":"models.local_only", "value":true}),
                    json!({"type":"select", "field":"models.talk", "value":"qwen3:4b"}),
                    json!({"type":"select", "field":"models.router", "value":"qwen3:0.6b"}),
                    json!({"type":"select", "field":"models.think", "value":"qwen3-coder:30b-64k"}),
                    json!({"type":"button", "event":"refresh"}),
                ],
            ),
            (
                GalleryPage::Fixture,
                "Fixture",
                vec![
                    json!({"type":"slider", "field":"volume", "value":50.0}),
                    json!({"type":"select", "field":"mode", "value":"normal"}),
                ],
            ),
        ];
        assert_eq!(GalleryPage::ALL, cases.each_ref().map(|case| case.0));
        let mut gallery = Gallery::default();
        assert_eq!(gallery.selected(), GalleryPage::Layout);
        for (page, name, expected) in cases {
            gallery.select(page);
            assert_eq!(gallery.selected(), page);
            assert_eq!(page.name(), name);
            let surface = gallery.surface().matrix_v1().expect("Matrix page");
            surface.validate().unwrap();
            let encoded = serde_json::to_vec(surface).unwrap();
            assert_eq!(mote_ui::v1::validate(&encoded).unwrap(), *surface);
            let value: Value = serde_json::from_slice(&encoded).unwrap();
            let mut all = Vec::new();
            nodes(&value["root"], &mut all);
            for node in &all {
                assert!(mote_ui::v1::catalog_inventory().iter().any(|entry| {
                    entry.name == node["type"].as_str().unwrap()
                        && entry.status == mote_ui::v1::ContractStatus::Frozen
                }));
            }
            for expected_node in expected {
                assert!(
                    all.iter().any(|node| expected_node
                        .as_object()
                        .unwrap()
                        .iter()
                        .all(|(key, value)| node[key] == *value)),
                    "{name} is missing {expected_node}"
                );
            }
            assert!(matches!(gallery.surface(), crate::Surface::V1 { .. }));
            mote_ui::validate(&serde_json::to_vec(gallery.surface().display_v0()).unwrap())
                .unwrap();
        }
    }

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
