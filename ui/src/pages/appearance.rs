#![warn(unused, dead_code)]
//! Port of `Components/Pages/Appearance.razor` — spec 06 §6.4. Owner: ui-pages-a.

use crate::common::markdown;
use crate::pages::new_session::set_title;
use crate::state::use_app;
use crate::{interop, ipc};
use ccui_shared::api::{FLAG_NOTIFY, FLAG_THINKING, FLAG_TIMESTAMPS, PREF_CODE_SIZE, PREF_THEME, THEMES};
use ccui_shared::{i18n, t};
use leptos::prelude::*;

/// Each language is named in itself.
const LANGUAGES: [(&str, &str); 2] = [("en", "English"), ("fr", "Français")];
const SIZES: [u32; 4] = [12, 13, 14, 15];

/// The code-size sample (a csharp fence, rendered by `md::render`).
pub fn sample() -> String {
    format!(
        "```csharp\n// {}\nMood MoodFor(Session s) => s.State switch\n{{\n    State.WaitingPermission => Mood.Shocked,\n    State.Running           => Mood.Excited,\n    State.Crashed           => Mood.Ko,\n    _                       => Mood.Happy,\n}};\n```",
        t!("Appearance.SampleComment")
    )
}

fn flag_set(name: &'static str, on: bool) {
    leptos::task::spawn_local(async move {
        if let Err(e) = ipc::app_flag_set(name, on).await {
            leptos::logging::error!("app_flag_set({name}): {e}");
        }
    });
}

fn pref_set(name: &'static str, value: String) {
    leptos::task::spawn_local(async move {
        if let Err(e) = ipc::app_pref_set(name, Some(&value)).await {
            leptos::logging::error!("app_pref_set({name}): {e}");
        }
    });
}

/// Language, theme cards, code size, notifications, thinking summaries, timestamps, version + update check.
#[component]
pub fn Appearance() -> impl IntoView {
    let app = use_app();
    let boot = app.boot.get_value();
    set_title(&t!("Settings.Title"));
    let culture = boot.culture.chars().take(2).collect::<String>();
    let pending = RwSignal::new(culture.clone()); // the language picked but not applied yet

    // Known from the first render (AppPref, kept current by PrefChanged: the palette's theme command or another
    // window): no card flashes unselected. A choice made before AppPref existed is read back from <html> once.
    let theme = Memo::new(move |_| app.prefs.with(|p| p.theme.clone()).unwrap_or_else(interop::get_theme));
    let size = Memo::new(move |_| app.prefs.with(|p| p.code_size.clone()).unwrap_or_else(interop::get_code_size));
    if boot.prefs.theme.is_none() {
        pref_set(PREF_THEME, interop::get_theme());
    }
    if boot.prefs.code_size.is_none() {
        pref_set(PREF_CODE_SIZE, interop::get_code_size());
    }

    let set_theme = move |k: &'static str| {
        app.prefs.update(|p| p.theme = Some(k.to_string()));
        interop::set_theme(k);
        pref_set(PREF_THEME, k.to_string());
    };
    let set_size = move |v: String| {
        app.prefs.update(|p| p.code_size = Some(v.clone()));
        interop::set_code_size(&v);
        pref_set(PREF_CODE_SIZE, v);
    };
    let flip = move |name: &'static str, get: fn(&ccui_shared::api::Flags) -> bool, set: fn(&mut ccui_shared::api::Flags, bool)| {
        let on = !app.flags.with_untracked(get);
        app.flags.update(|f| set(f, on));
        flag_set(name, on);
    };

    let apply = {
        let culture = culture.clone();
        move |_| {
            let c = pending.get_untracked();
            if c == culture {
                return;
            }
            leptos::task::spawn_local(async move {
                if let Err(e) = ipc::set_culture(&c).await {
                    leptos::logging::error!("set_culture: {e}");
                    return;
                }
                let mut b = app.boot.get_value();
                b.culture = c;
                ipc::save_boot_mirror(&b);
                if let Some(w) = web_sys::window() {
                    let _ = w.location().reload();
                }
            });
        }
    };

    let themes = THEMES
        .iter()
        .map(|&k| {
            let n = t!(&format!("Theme.{k}"));
            let on = move || theme.get() == k;
            view! {
                // data-theme scopes the preview to the card's own tokens
                <button class=move || if on() { "tc on" } else { "tc" } data-theme=k role="radio" aria-checked=move || if on() { "true" } else { "false" }
                    tabindex=move || if on() { 0 } else { -1 } aria-label=t!("Appearance.ThemeLabel", n.clone()) on:click=move |_| set_theme(k)>
                    <div class="pv">
                        <div class="rl"><i></i><i></i><i></i><i></i></div>
                        <div class="bd">
                            <div class="ln"><span style="color:var(--read)">"Read"</span>" "<span style="color:var(--fg-2)">"Store.cs"</span></div>
                            <div class="ln"><span style="color:var(--edit)">"Edit"</span>" "<span style="color:var(--fg-2)">"Home.razor"</span></div>
                            <div class="ln"><span style="color:var(--syn-kw)">"new"</span>" "<span style="color:var(--syn-type)">"Store"</span><span style="color:var(--fg-3)">"("</span><span style="color:var(--syn-str)">"\"cwd\""</span><span style="color:var(--fg-3)">")"</span></div>
                            <div class="ln" style="color:var(--syn-com);font-style:italic">"// "{n.to_lowercase()}</div>
                            <span class="btnp">{t!("Perm.Allow")}</span>
                        </div>
                    </div>
                    <div class="nm"><b>{n.clone()}</b><span class="sw"><i style="background:var(--syn-kw)"></i><i style="background:var(--syn-str)"></i><i style="background:var(--syn-fn)"></i><i style="background:var(--syn-type)"></i></span></div>
                </button>
            }
        })
        .collect_view();

    let sizes = SIZES
        .iter()
        .map(|&px| {
            let v = format!("{px}px");
            let on = {
                let v = v.clone();
                move || size.get() == v
            };
            let on2 = on.clone();
            let on3 = on.clone();
            view! {
                <button class=move || on().then_some("on") role="radio" aria-checked=move || if on2() { "true" } else { "false" }
                    tabindex=move || if on3() { 0 } else { -1 } aria-label=format!("{px} px") on:click=move |_| set_size(v.clone())>{px}</button>
            }
        })
        .collect_view();

    let toggle = move |on: Signal<bool>, id: Option<&'static str>, describedby: &'static str, label: String, click: Box<dyn Fn() + Send + Sync>| {
        view! {
            <button id=id class=move || if on.get() { "toggle on" } else { "toggle" } role="switch" tabindex="0"
                aria-checked=move || if on.get() { "true" } else { "false" } aria-describedby=describedby
                on:click=move |_| click()><span class="sw"></span>{label}</button>
        }
    };
    let changed = {
        let culture = culture.clone();
        Signal::derive(move || pending.get() != culture)
    };
    let notify = Signal::derive(move || app.flags.with(|f| f.notify));
    let thinking = Signal::derive(move || app.flags.with(|f| f.thinking));
    let stamps = Signal::derive(move || app.flags.with(|f| f.timestamps));
    // Only the macOS shell learns about a click: elsewhere the hint promises none.
    let notify_hint = if boot.mac { "Appearance.NotifyHintMac" } else { "Appearance.NotifyHintDesktop" };

    view! {
        <div class="pane">
            <div class="ph"><h1 class="ttl">{t!("Settings.Title")}</h1></div>
            <div class="settings">
                <section class="set-grp" aria-labelledby="g-look">
                    <h2 id="g-look">{t!("Settings.Look")}</h2>
                    <div class="set-row">
                        <div><label for="lang"><b>{t!("Appearance.Language")}</b></label><span id="h-lang">{t!("Appearance.LanguageHint")}</span></div>
                        // Choosing an option changes nothing until Apply: set_culture, then a reload.
                        <div class="lang">
                            <select id="lang" name="c" aria-describedby="h-lang" on:change=move |e| pending.set(event_target_value(&e))>
                                {LANGUAGES.iter().map(|&(c, name)| view! { <option value=c lang=c selected=c == culture>{name}</option> }).collect_view()}
                            </select>
                            <button class=move || if changed.get() { "btn primary" } else { "btn" } type="submit" tabindex="0"
                                disabled=move || !changed.get() on:click=apply>{t!("Appearance.Apply")}</button>
                        </div>
                    </div>
                    <div class="set-row">
                        <div><b id="l-theme">{t!("Appearance.Theme")}</b><span id="h-theme" inner_html=i18n::kbd(&t!("Appearance.ThemeHint"))></span></div>
                        // One Tab stop, arrows move and choose (radiogroup keys in interop.js).
                        <div class="themes" role="radiogroup" aria-labelledby="l-theme" aria-describedby="h-theme">{themes}</div>
                    </div>
                    <div class="set-row">
                        <div><b id="l-size">{t!("Appearance.CodeSize")}</b><span id="h-size">{t!("Appearance.CodeSizeHint")}</span></div>
                        <div class="size">
                            <div class="seg" role="radiogroup" aria-labelledby="l-size" aria-describedby="h-size">{sizes}</div>
                            <span class="unit" aria-hidden="true">"px"</span>
                            // Right under its control: code size is judged by looking. A picture, not a control: no Copy
                            // or scroller in the Tab order.
                            <div class="sample md" inert inner_html=markdown::rendered(&sample())></div>
                        </div>
                    </div>
                </section>
                <section class="set-grp" aria-labelledby="g-sessions">
                    <h2 id="g-sessions">{t!("Settings.Sessions")}</h2>
                    <div class="set-row">
                        <div><b>{t!("Appearance.Notify")}</b><span id="h-notify">{t!(notify_hint)}</span></div>
                        <div>{toggle(notify, Some("notify"), "h-notify", t!("Appearance.NotifyLabel"),
                            Box::new(move || flip(FLAG_NOTIFY, |f| f.notify, |f, v| f.notify = v)))}</div>
                    </div>
                    // Backend flags (AppFlag): claude's launch arguments read one.
                    <div class="set-row">
                        <div><b>{t!("Appearance.Thinking")}</b><span id="h-thinking">{t!("Appearance.ThinkingHint")}</span></div>
                        <div>{toggle(thinking, None, "h-thinking", t!("Appearance.ThinkingLabel"),
                            Box::new(move || flip(FLAG_THINKING, |f| f.thinking, |f, v| f.thinking = v)))}</div>
                    </div>
                    <div class="set-row">
                        <div><b>{t!("Appearance.Stamps")}</b><span id="h-stamps">{t!("Appearance.StampsHint")}</span></div>
                        <div>{toggle(stamps, None, "h-stamps", t!("Appearance.StampsLabel"),
                            Box::new(move || flip(FLAG_TIMESTAMPS, |f| f.timestamps, |f, v| f.timestamps = v)))}</div>
                    </div>
                </section>
                <section class="set-grp" aria-labelledby="g-about">
                    <h2 id="g-about">{t!("Settings.About")}</h2>
                    <div class="set-row">
                        <div><b>{t!("Appearance.Version")}</b><span>{t!("Appearance.VersionHintDesktop")}</span></div>
                        // The shell runs the check and answers in a native dialog.
                        <div class="lang"><code>"Claude Code UI "{boot.version.clone()}</code><button class="btn" type="button" tabindex="0" on:click=move |_| {
                            leptos::task::spawn_local(async {
                                if let Err(e) = ipc::check_for_updates().await {
                                    leptos::logging::error!("check_for_updates: {e}");
                                }
                            });
                        }>{t!("Appearance.UpdateCheck")}</button></div>
                    </div>
                </section>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_is_one_fence() {
        let s = sample();
        assert!(s.starts_with("```csharp\n// ") && s.ends_with("};\n```"));
        assert_eq!(s.matches("```").count(), 2);
    }
}
