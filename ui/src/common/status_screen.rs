//! Port of `Components/Shared/StatusScreen.razor` — spec 06 §6.8. Owner: ui-shell.
//! Full-pane status page (404 router fallback; 4xx / 5xx kept for a panel shown on a failure): the .crash card
//! centred in the thread column.

use crate::common::duck::{Duck, Mood};
use crate::common::icon::Icon;
use ccui_shared::t;
use leptos::prelude::*;

/// Resource key prefix and duck mood of a status code.
pub fn kind(code: u16) -> (&'static str, Mood) {
    match code {
        404 => ("NotFound", Mood::Sad),
        c if c < 500 => ("BadRequest", Mood::Shocked),
        _ => ("Error", Mood::Ko),
    }
}

/// 404 (and optional 500 panel) screen: duck, `{key}.Title/Heading/Text`, path, actions.
#[component]
pub fn StatusScreen(
    code: u16,
    path: String,
    #[prop(optional)] request_id: Option<String>,
) -> impl IntoView {
    let (key, mood) = kind(code);
    let title = t!(&format!("{key}.Title"));
    if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
        doc.set_title(&format!("{title} · Claude Code"));
    }
    let request_id = request_id.filter(|r| !r.is_empty());
    let line = if path.is_empty() { code.to_string() } else { format!("{code} · {path}") };
    let acts = if code >= 500 {
        view! {
            {(!path.is_empty()).then(|| view! {
                <a class="btn primary" href=path.clone()><Icon name="refresh" sm=true/>{t!("Layout.Reload")}</a>
            })}
            <a class=if path.is_empty() { "btn primary" } else { "btn" } href=""><Icon name="plus" sm=true/>{t!("Rail.NewSession")}</a>
        }
        .into_any()
    } else {
        view! {
            <a class="btn primary" href=""><Icon name="plus" sm=true/>{t!("Rail.NewSession")}</a>
            <a class="btn" href="overview">{t!("Nav.Overview")}</a>
        }
        .into_any()
    };
    view! {
        <div class="pane">
            <div class="ph"><h1 class="ttl" tabindex="-1">{title}</h1></div>
            <div class="oops">
                <div class=if code == 404 { "crash calm" } else { "crash" } role=(code >= 500).then_some("alert")>
                    <div class="duck" aria-hidden="true"><Duck mood=mood size=72/></div>
                    <div>
                        <p class="code">{line}</p>
                        <h2>{t!(&format!("{key}.Heading"))}</h2>
                        <p>{t!(&format!("{key}.Text"))}</p>
                        {request_id.clone().map(|id| view! { <pre><span>{t!("Error.RequestId")}</span>" "{id}</pre> })}
                        <div class="acts">
                            {acts}
                            {request_id.map(|id| view! {
                                <button type="button" class="btn ghost" data-copy=id><Icon name="copy" sm=true/>{t!("Error.CopyId")}</button>
                            })}
                        </div>
                    </div>
                </div>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds() {
        assert_eq!(kind(404), ("NotFound", Mood::Sad));
        assert_eq!(kind(400), ("BadRequest", Mood::Shocked));
        assert_eq!(kind(500), ("Error", Mood::Ko));
        for k in ["NotFound", "BadRequest", "Error"] {
            for part in ["Title", "Heading", "Text"] {
                assert!(ccui_shared::i18n::has(&format!("{k}.{part}")), "{k}.{part}");
            }
        }
    }
}
