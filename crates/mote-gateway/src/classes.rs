#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Full,
    Watch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Own,
    Read,
    Control,
    MacOnly,
}

pub fn class_of(method: &str) -> Option<Class> {
    match method {
        "hello" | "state" | "events.subscribe" | "surface.get" => Some(Class::Read),
        "chat.send" | "ui.event" | "approval.decide" => Some(Class::Control),
        "gateway.pair_start" | "gateway.device_revoke" | "gateway.enable" | "gateway.disable" => {
            Some(Class::MacOnly)
        }
        _ => None,
    }
}

pub fn allowed(method: &str, scope: Scope) -> anyhow::Result<Class> {
    match class_of(method) {
        None => anyhow::bail!("unknown method"),
        Some(Class::MacOnly) => anyhow::bail!("local owner only"),
        Some(Class::Control) if scope == Scope::Watch => anyhow::bail!("watch only"),
        Some(class) => Ok(class),
    }
}
