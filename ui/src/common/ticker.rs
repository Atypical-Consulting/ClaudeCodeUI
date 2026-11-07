//! Port of `Components/Shared/Ticker.razor` — spec 06 §7, 07 §0.4.8. Owner: ui-shell.
//! Re-renders only its content, once a second while active: elapsed times move without events.

use leptos::prelude::*;
use std::time::Duration;

/// Re-renders its children once a second while `active` (set_interval driving a tick signal the children
/// read; cleared when inactive or unmounted).
#[component]
pub fn Ticker(
    #[prop(into)] active: Signal<bool>,
    children: ChildrenFn,
) -> impl IntoView {
    let tick = RwSignal::new(0u32);
    let timer: StoredValue<Option<IntervalHandle>> = StoredValue::new(None);
    let stop = move || {
        if let Some(h) = timer.try_update_value(Option::take).flatten() {
            h.clear();
        }
    };
    Effect::new(move |_| {
        let on = active.get();
        stop();
        if on {
            if let Ok(h) = set_interval_with_handle(move || tick.update(|t| *t = t.wrapping_add(1)), Duration::from_secs(1)) {
                timer.set_value(Some(h));
            }
        }
    });
    on_cleanup(stop);
    move || {
        tick.track();
        children()
    }
}
