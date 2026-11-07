//! Port of `Components/Shared/EffortSlider.razor` — spec 06 §7. Owner: ui-shell.
//! Ordered levels on a native range (arrows, Home/End, touch and AT come for free), their names as clickable ticks
//! under it. Equal columns keep each tick centred under its thumb position (app.css).

use leptos::prelude::*;
use std::sync::atomic::{AtomicU32, Ordering};

static SEQ: AtomicU32 = AtomicU32::new(0);

/// Index of `value` among the first `reach` stops, -1 when the range does not hold it (unknown effort: no tick).
pub fn index(stops: &[String], value: &str, reach: usize) -> i32 {
    stops.iter().take(reach).position(|s| s == value).map_or(-1, |i| i as i32)
}

/// `--p`: the thumb's fraction, invariant `0.###` (0 for a single stop).
pub fn fraction(shown: i32, reach: usize) -> String {
    if reach <= 1 {
        return "0".into();
    }
    let p = shown.max(0) as f64 / (reach - 1) as f64;
    let s = format!("{p:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() { "0".into() } else { s.to_string() }
}

/// Range input + tick buttons over `levels`; drag state; `on_change(level)` on release.
#[component]
pub fn EffortSlider(
    levels: Vec<String>,
    #[prop(into)] value: Signal<String>,
    on_change: Callback<String>,
    name: Callback<String, String>,
    #[prop(optional, into)] id: Option<String>,
    #[prop(optional, into)] label: Option<String>,
) -> impl IntoView {
    let id = id.unwrap_or_else(|| format!("effort-{}", SEQ.fetch_add(1, Ordering::Relaxed) + 1));
    let reach = levels.len();
    let levels = StoredValue::new(levels);
    // The thumb while dragging: the fill and the tick follow it before the change commits.
    let drag = RwSignal::new(None::<i32>);
    let at = move || levels.with_value(|l| index(l, &value.get(), reach));
    let shown = move || drag.get().unwrap_or_else(at);
    let level = move |i: i32| levels.with_value(|l| l.get(i as usize).cloned());
    // Dragged back to where it started, release fires no change: drop drag now, or it would mask every later value.
    let on_drag = move |i: i32| drag.set(if i == at() { None } else { Some(i) });
    let pick = move |i: i32| {
        if i < 0 || i as usize >= reach {
            return;
        }
        drag.set(Some(i));
        if let Some(l) = level(i) {
            if l != value.get_untracked() {
                on_change.run(l);
            }
        }
        drag.set(None); // the parent has applied it (or reported the failure): show its value again
    };
    let parse = |e: &leptos::ev::Event| event_target_value(e).parse::<i32>().unwrap_or(-1);
    let ticks = (0..reach as i32)
        .map(|n| {
            view! {
                <button type="button" tabindex="-1" aria-hidden="true" class=move || (n == shown()).then_some("on")
                    on:click=move |_| pick(n)>{move || level(n).map(|l| name.run(l))}</button>
            }
        })
        .collect_view();
    view! {
        <span class="effort-slider" style=format!("--n:{reach};--k:{reach}")>
            <input type="range" id=id min="0" max=reach.saturating_sub(1).to_string() step="1"
                prop:value=move || shown().to_string() style=move || format!("--p:{}", fraction(shown(), reach))
                aria-label=label aria-valuetext=move || { let s = shown(); (s >= 0).then(|| level(s).map(|l| name.run(l))).flatten() }
                on:input=move |e| on_drag(parse(&e)) on:change=move |e| pick(parse(&e))/>
            // The range already carries name and value for AT and the keyboard: the ticks are a mouse shortcut.
            {ticks}
        </span>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_and_fraction() {
        let l: Vec<String> = ["low", "medium", "high", "max"].map(String::from).to_vec();
        assert_eq!(index(&l, "high", 4), 2);
        assert_eq!(index(&l, "max", 3), -1);
        assert_eq!(index(&l, "xhigh", 4), -1);
        assert_eq!(fraction(2, 4), "0.667");
        assert_eq!(fraction(3, 4), "1");
        assert_eq!(fraction(0, 4), "0");
        assert_eq!(fraction(-1, 4), "0");
        assert_eq!(fraction(1, 3), "0.5");
        assert_eq!(fraction(0, 1), "0");
    }
}
