//! Port of `Components/Layout/QuotaMeter.razor` — spec 06 §4 QuotaMeter. Owner: ui-shell.
//! Quota from the latest rate_limit_event or get_usage; nothing until one arrived. Per-model weekly windows, usage
//! credits and, on demand, the week's breakdown (/usage) come from get_usage.

use crate::common::icon::Icon;
use crate::ipc;
use crate::state::use_app;
use ccui_shared::usage::UsageRow;
use ccui_shared::{fmt, i18n, t};
use leptos::prelude::*;

/// `W(f)` = round(clamp(f, 0, 1) * 100) + "%".
pub fn width(f: f64) -> String {
    // Math.Round is banker's rounding.
    format!("{}%", fmt::round_half_even(f.clamp(0.0, 1.0) * 100.0))
}

fn row_view(r: UsageRow) -> impl IntoView {
    view! {
        <div class="qrow wide"><span title=r.name.clone()>{r.name.clone()}</span><span class="bar"><i style=format!("width:{}", width(r.pct))></i></span><span>{fmt::pct(r.pct)}</span></div>
    }
}

/// 5 h / 7 d windows, per-model rows, credits, warning, weekly breakdown (refresh_usage(behaviors)).
/// Only when the summary has limits.
#[component]
pub fn QuotaMeter() -> impl IntoView {
    let app = use_app();
    let open = RwSignal::new(false);
    let loading = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let limits = Memo::new(move |_| app.summary.with(|s| s.limits.clone()));
    let source = Memo::new(move |_| app.summary.with(|s| s.usage_source.clone()));

    // The breakdown scans every local transcript of the week: read when opened, not at each session start.
    let toggle = move |_| {
        open.update(|o| *o = !*o);
        let Some(src) = source.get_untracked().filter(|s| s.has_process) else { return };
        if !open.get_untracked() {
            return;
        }
        loading.set(true);
        error.set(None);
        leptos::task::spawn_local(async move {
            if let Err(e) = ipc::session_refresh_usage(&src.session_id, true).await {
                let _ = error.try_set(Some(t!("Quota.ReadFailed", e)));
            }
            let _ = loading.try_set(false);
        });
    };

    move || {
        let l = limits.get()?;
        let src = source.get();
        let u = src.as_ref().map(|s| s.usage.clone());
        let has_process = src.as_ref().is_some_and(|s| s.has_process);
        let hint = t!("Quota.Hint", l.seven_day_reset.map(fmt::short_date_time).unwrap_or_default());
        let resets = t!("Quota.Resets", l.five_hour_reset.map(fmt::time).unwrap_or_default());
        let models = u.as_ref().map(|u| u.models.clone()).unwrap_or_default();
        let credits = u.as_ref().and_then(|u| {
            let spent = u.spent?;
            Some(match u.spend_limit {
                Some(lim) => format!("{} / {}", u.money(spent), u.money(lim)),
                None => t!("Quota.Used", u.money(spent)),
            })
        });
        // The CLI's limit banner: the limiting window, and whether extra usage takes over.
        let warn = matches!(l.status.as_str(), "allowed_warning" | "rejected").then(|| {
            let rejected = l.status == "rejected";
            let window = i18n::name("Quota.Window.", Some(l.kind.as_deref().unwrap_or("seven_day")));
            let extra = if l.extra_in_use {
                "Quota.ExtraInUse"
            } else if l.extra_on || u.as_ref().and_then(|u| u.extra_on) == Some(true) {
                "Quota.ExtraOn"
            } else {
                "Quota.ExtraOff"
            };
            view! {
                <p class=if rejected { "qwarn c-err-fg" } else { "qwarn" }>
                    {t!(if rejected { "Quota.Blocked" } else { "Quota.Warn" }, window)}" "{t!(extra)}" "
                    <a href="https://claude.ai/settings/usage" target="_blank" rel="noopener noreferrer">{t!("Quota.Settings")}</a>
                </p>
            }
        });
        let week = move || {
            if let Some(e) = error.get() {
                return view! { <p class="c-err-fg" role="alert">{e}</p> }.into_any();
            }
            if loading.get() {
                return view! { <p role="status">{t!("Quota.Reading")}</p> }.into_any();
            }
            let Some(u) = source.get().map(|s| s.usage) else { return ().into_any() };
            let empty = u.surfaces.is_empty() && u.week.is_empty();
            let surfaces = (!u.surfaces.is_empty()).then(|| {
                view! { <span class="cap">{t!("Quota.Surfaces")}</span>{u.surfaces.into_iter().map(row_view).collect_view()} }
            });
            let groups = u
                .week
                .into_iter()
                .map(|g| {
                    let beh = g.kind == "behaviors";
                    let rows = g
                        .rows
                        .into_iter()
                        .map(|r| if beh { UsageRow { name: i18n::name("Quota.Beh.", Some(&r.name)), ..r } } else { r })
                        .map(row_view)
                        .collect_view();
                    view! { <span class="cap">{i18n::name("Quota.Kind.", Some(&g.kind))}</span>{rows} }
                })
                .collect_view();
            view! { {surfaces}{groups}{empty.then(|| view! { <p>{t!("Quota.NoBreakdown")}</p> })} }.into_any()
        };
        Some(view! {
            <div class="quota" title=hint>
                <div class="qh"><span class="cap">{t!("Quota.Usage")}</span><span>{resets}</span></div>
                <div class="qrow"><span>{t!("Quota.FiveHours")}</span><span class="bar"><i style=format!("width:{}", width(l.five_hour))></i></span><span>{fmt::pct(l.five_hour)}</span></div>
                <div class="qrow"><span>{t!("Quota.SevenDays")}</span><span class="bar"><i style=format!("width:{}", width(l.seven_day))></i></span><span>{fmt::pct(l.seven_day)}</span></div>
                {models.into_iter().map(|m| view! {
                    <div class="qrow"><span title=t!("Quota.ModelWeek", m.name)>{m.name.clone()}</span><span class="bar"><i style=format!("width:{}", width(m.pct))></i></span><span>{fmt::pct(m.pct)}</span></div>
                }).collect_view()}
                {credits.map(|c| view! { <div class="qrow kv"><span>{t!("Quota.Credits")}</span><span>{c}</span></div> })}
                <div role="status">{warn}</div>
                {has_process.then(|| view! {
                    <button type="button" class="btn ghost qmore" aria-expanded=move || if open.get() { "true" } else { "false" }
                        aria-controls="quota-week" on:click=toggle>{t!("Quota.Breakdown")}<Icon name="down" sm=true/></button>
                })}
                <Show when=move || open.get()>
                    <div id="quota-week" class="qweek" aria-busy=move || if loading.get() { "true" } else { "false" }>{week}</div>
                </Show>
            </div>
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widths() {
        assert_eq!(width(0.5), "50%");
        assert_eq!(width(1.7), "100%");
        assert_eq!(width(-0.2), "0%");
        assert_eq!(width(0.125), "12%"); // 12.5 rounds to even
        assert_eq!(width(0.135), "14%");
    }
}
