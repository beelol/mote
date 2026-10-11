use crate::surface::SurfaceEvent;

pub fn discard_change(ctx: &egui::Context, event: &SurfaceEvent) {
    let SurfaceEvent::Change { state_id, .. } = event else {
        return;
    };
    if event.context().surface_contract == Some(1) {
        crate::surface_v1::discard_change(ctx, *state_id);
        return;
    }
    ctx.data_mut(|data| {
        data.remove::<(bool, bool)>(*state_id);
        data.remove::<(String, String)>(*state_id);
    });
}
