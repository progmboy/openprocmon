//! The "Save To File" dialog (design `SaveDialog` in gui-design-v2).
//!
//! Lets the user choose which events to save (all / filtered / highlighted), the
//! output format (PML / CSV / XML), and the destination path, then writes them.
//! Built on the shared [`FormDialog`](super::form_dialog::FormDialog) chrome; the
//! radio/check rows are bespoke to match the design's `.rc-row` styling. Only PML
//! writing is implemented; CSV/XML are surfaced but report "not implemented yet".

use gpui::{
    div, prelude::FluentBuilder, px, App, AppContext, Context, Div, Entity, Hsla,
    InteractiveElement, IntoElement, ParentElement, SharedString, StatefulInteractiveElement,
    Styled, WeakEntity, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants},
    h_flex,
    input::{Input, InputState},
    v_flex, ActiveTheme, Icon, StyledExt, WindowExt,
};
use rust_i18n::t;

use crate::app::AppView;
use crate::icons::PmIcon;
use crate::theme::palette;

/// Which events to save.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SaveScope {
    All,
    Filtered,
    Highlighted,
}

/// Output format.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SaveFormat {
    Pml,
    Csv,
    Xml,
}

impl SaveFormat {
    fn ext(self) -> &'static str {
        match self {
            SaveFormat::Pml => "PML",
            SaveFormat::Csv => "CSV",
            SaveFormat::Xml => "XML",
        }
    }
}

/// Per-scope event counts shown next to the radios and in the footer.
#[derive(Clone, Copy, Default)]
pub(crate) struct SaveCounts {
    pub total: u64,
    pub filtered: u64,
    pub highlighted: u64,
}

/// The chosen options, handed to [`AppView`] when the user confirms.
#[derive(Clone)]
pub(crate) struct SaveOptions {
    pub scope: SaveScope,
    pub profiling: bool,
    pub format: SaveFormat,
    pub stacks: bool,
    pub path: String,
}

pub(crate) struct SaveDialog {
    app: WeakEntity<AppView>,
    path: Entity<InputState>,
    scope: SaveScope,
    profiling: bool,
    format: SaveFormat,
    stacks: bool,
    symbols: bool,
    counts: SaveCounts,
}

impl SaveDialog {
    pub(crate) fn new(app: WeakEntity<AppView>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Default to a full path (current directory + Logfile.PML) so the field
        // shows the complete destination rather than a bare file name.
        let default_path = std::env::current_dir()
            .unwrap_or_default()
            .join("Logfile.PML")
            .to_string_lossy()
            .to_string();
        let path = cx.new(|cx| {
            let mut s = InputState::new(window, cx);
            s.set_value(&default_path, window, cx);
            s
        });
        Self {
            app,
            path,
            scope: SaveScope::Filtered,
            profiling: true,
            format: SaveFormat::Pml,
            stacks: false,
            symbols: false,
            counts: SaveCounts::default(),
        }
    }

    /// Seeds the dialog's live counts when opening (path keeps its current value).
    pub(crate) fn load(&mut self, counts: SaveCounts) {
        self.counts = counts;
    }

    fn scope_count(&self) -> u64 {
        match self.scope {
            SaveScope::All => self.counts.total,
            SaveScope::Filtered => self.counts.filtered,
            SaveScope::Highlighted => self.counts.highlighted,
        }
    }

    /// Switches format and rewrites the path extension (cf. design `setFmt`).
    fn set_format(&mut self, format: SaveFormat, window: &mut Window, cx: &mut Context<Self>) {
        self.format = format;
        if format != SaveFormat::Xml {
            self.stacks = false;
            self.symbols = false;
        }
        let cur = self.path.read(cx).value().to_string();
        let new = match cur.rfind('.') {
            Some(i) if cur[i + 1..].eq_ignore_ascii_case("PML")
                || cur[i + 1..].eq_ignore_ascii_case("CSV")
                || cur[i + 1..].eq_ignore_ascii_case("XML") =>
            {
                format!("{}.{}", &cur[..i], format.ext())
            }
            _ => format!("{cur}.{}", format.ext()),
        };
        self.path.update(cx, |s, cx| s.set_value(&new, window, cx));
        cx.notify();
    }

    fn confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let opts = SaveOptions {
            scope: self.scope,
            profiling: self.profiling,
            format: self.format,
            stacks: self.stacks,
            path: self.path.read(cx).value().to_string(),
        };
        self.app.update(cx, |view, cx| view.save_to_file(opts, window, cx)).ok();
        window.close_dialog(cx);
    }

    /// The dialog footer: "Will save N events" on the left, Cancel + OK on the right.
    /// The count is read when the footer is built (the dialog's default scope).
    pub(crate) fn footer(dialog: &Entity<SaveDialog>, cx: &App) -> impl IntoElement {
        let n = dialog.read(cx).scope_count();
        let d_ok = dialog.clone();
        h_flex()
            .w_full()
            .items_center()
            .gap_2()
            .child(
                div()
                    .text_size(px(11.5))
                    .text_color(cx.theme().muted_foreground)
                    .child(t!("dlg.save_footer", n = group_thousands(n)).to_string()),
            )
            .child(div().flex_1())
            .child(
                Button::new("save-cancel")
                    .h(px(34.))
                    .label(t!("dlg.cancel").to_string())
                    .on_click(move |_, window, cx| {
                        window.close_dialog(cx);
                    }),
            )
            .child(
                Button::new("save-ok")
                    .primary()
                    .h(px(34.))
                    .icon(PmIcon::Check)
                    .label(t!("dlg.ok").to_string())
                    .on_click(move |_, window, cx| {
                        d_ok.update(cx, |this, cx| this.confirm(window, cx));
                    }),
            )
    }
}

/// The dialog body colors, mapped from the active theme + palette (design vars).
struct Co {
    fg: Hsla,
    muted: Hsla,
    faint: Hsla,
    border: Hsla,
    bg2: Hsla,
    hover: Hsla,
    accent: Hsla,
}

impl Co {
    fn new(cx: &App) -> Self {
        Self {
            fg: cx.theme().foreground,
            muted: cx.theme().muted_foreground,
            faint: cx.theme().muted_foreground.opacity(0.55),
            border: cx.theme().border,
            bg2: cx.theme().title_bar,
            hover: cx.theme().list_hover,
            accent: palette(cx).row_sel_bar,
        }
    }
}

impl gpui::Render for SaveDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let co = Co::new(cx);
        let xml = self.format == SaveFormat::Xml;

        v_flex()
            .w_full()
            .px(px(22.))
            .py(px(20.))
            // --- Events to save ---
            .child(group_label(t!("dlg.save_events").to_string(), &co))
            .child(
                v_flex()
                    .gap(px(1.))
                    .child(self.radio_row(
                        "sc-all",
                        SaveScope::All,
                        t!("dlg.save_all").to_string(),
                        Some(self.counts.total),
                        &co,
                        cx,
                    ))
                    .child(self.radio_row(
                        "sc-flt",
                        SaveScope::Filtered,
                        t!("dlg.save_filtered").to_string(),
                        Some(self.counts.filtered),
                        &co,
                        cx,
                    ))
                    .child(
                        div().pl(px(30.)).child(self.check_row(
                            "sc-prof",
                            self.profiling,
                            self.scope != SaveScope::Filtered,
                            t!("dlg.save_profiling").to_string(),
                            &co,
                            cx,
                            |this| this.profiling = !this.profiling,
                        )),
                    )
                    .child(self.radio_row(
                        "sc-hl",
                        SaveScope::Highlighted,
                        t!("dlg.save_highlighted").to_string(),
                        Some(self.counts.highlighted),
                        &co,
                        cx,
                    )),
            )
            // --- Format ---
            .child(div().mt(px(18.)).child(group_label(t!("dlg.save_format").to_string(), &co)))
            .child(
                v_flex()
                    .gap(px(1.))
                    .child(self.format_row("fm-pml", SaveFormat::Pml, t!("dlg.save_pml").to_string(), &co, cx))
                    .child(self.format_row("fm-csv", SaveFormat::Csv, t!("dlg.save_csv").to_string(), &co, cx))
                    .child(self.format_row("fm-xml", SaveFormat::Xml, t!("dlg.save_xml").to_string(), &co, cx))
                    .child(
                        v_flex()
                            .pl(px(30.))
                            .gap(px(1.))
                            .child(self.check_row(
                                "fm-stk",
                                self.stacks,
                                !xml,
                                t!("dlg.save_stacks").to_string(),
                                &co,
                                cx,
                                |this| this.stacks = !this.stacks,
                            ))
                            .child(self.check_row(
                                "fm-sym",
                                self.symbols,
                                !xml || !self.stacks,
                                t!("dlg.save_symbols").to_string(),
                                &co,
                                cx,
                                |this| this.symbols = !this.symbols,
                            )),
                    ),
            )
            // --- Path ---
            .child(
                h_flex()
                    .mt(px(20.))
                    .items_center()
                    .gap(px(11.))
                    .child(div().text_size(px(12.5)).text_color(co.fg).flex_shrink_0().child(t!("dlg.save_path").to_string()))
                    .child(div().flex_1().child(Input::new(&self.path).map(|mut i| {
                        i.style().size.height = Some(px(34.).into());
                        i
                    })))
                    .child(
                        Button::new("save-browse")
                            .h(px(34.))
                            .w(px(40.))
                            .label("…")
                            .on_click(cx.listener(|this, _, window, cx| this.browse(window, cx))),
                    ),
            )
    }
}

impl SaveDialog {
    /// A scope radio row (`.rc-row` with a trailing count badge).
    fn radio_row(
        &self,
        id: &'static str,
        scope: SaveScope,
        label: String,
        count: Option<u64>,
        co: &Co,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let on = self.scope == scope;
        let text = co.fg;
        rc_row(id, on, false, text, co.hover)
            .child(radio_mark(on, co))
            .child(
                h_flex().flex_1().items_center().gap(px(8.)).child(div().child(label)).when_some(
                    count,
                    |this, n| this.child(count_badge(n, co)),
                ),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.scope = scope;
                cx.notify();
            }))
    }

    /// A format radio row (no count badge).
    fn format_row(
        &self,
        id: &'static str,
        format: SaveFormat,
        label: String,
        co: &Co,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let on = self.format == format;
        rc_row(id, on, false, co.fg, co.hover)
            .child(radio_mark(on, co))
            .child(div().flex_1().child(label))
            .on_click(cx.listener(move |this, _, window, cx| this.set_format(format, window, cx)))
    }

    /// A checkbox row. `toggle` mutates the relevant flag.
    #[allow(clippy::too_many_arguments)]
    fn check_row(
        &self,
        id: &'static str,
        on: bool,
        disabled: bool,
        label: String,
        co: &Co,
        cx: &mut Context<Self>,
        toggle: fn(&mut SaveDialog),
    ) -> impl IntoElement {
        let text = if disabled { co.faint } else { co.fg };
        let mut row = rc_row(id, on, disabled, text, co.hover)
            .child(check_mark(on, disabled, co))
            .child(div().flex_1().child(label));
        if !disabled {
            row = row.on_click(cx.listener(move |this, _, _, cx| {
                toggle(this);
                cx.notify();
            }));
        }
        row
    }

    /// Browse for a destination path via the native save dialog (cf. File ▸ Open).
    /// The picker runs on its own thread (a modal loop must not run on gpui's main
    /// thread); the result is applied back to the path input on the UI loop.
    fn browse(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let cur = self.path.read(cx).value().to_string();
        let (tx, rx) = crossbeam_channel::bounded::<Option<std::path::PathBuf>>(1);
        std::thread::spawn(move || {
            let name = cur.rsplit(['\\', '/']).next().unwrap_or("Logfile.PML").to_string();
            let _ = tx.send(rfd::FileDialog::new().set_file_name(name).save_file());
        });
        cx.spawn_in(window, async move |this, cx| {
            let picked = loop {
                match rx.try_recv() {
                    Ok(p) => break p,
                    Err(crossbeam_channel::TryRecvError::Empty) => {
                        cx.background_executor()
                            .timer(std::time::Duration::from_millis(30))
                            .await;
                    }
                    Err(_) => break None,
                }
            };
            if let Some(p) = picked {
                let _ = this.update_in(cx, |this, window, cx| {
                    this.path.update(cx, |s, cx| {
                        s.set_value(p.to_string_lossy().to_string(), window, cx)
                    });
                });
            }
        })
        .detach();
    }
}

/// A `.rc-row`: 7px×8px padding, 7px radius, hover bg (unless disabled).
fn rc_row(id: &'static str, _on: bool, disabled: bool, text: Hsla, hover: Hsla) -> gpui::Stateful<Div> {
    let mut row = div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(10.))
        .px(px(8.))
        .py(px(7.))
        .rounded(px(7.))
        .text_size(px(12.5))
        .text_color(text);
    if disabled {
        row = row.opacity(0.7);
    } else {
        row = row.cursor_pointer().hover(move |s| s.bg(hover));
    }
    row
}

/// A radio mark (`.rc-radio`): 17px ring, accent dot when on.
fn radio_mark(on: bool, co: &Co) -> impl IntoElement {
    div()
        .size(px(17.))
        .flex_shrink_0()
        .rounded_full()
        .border_2()
        .border_color(if on { co.accent } else { co.faint })
        .flex()
        .items_center()
        .justify_center()
        .when(on, |d| d.child(div().size(px(8.)).rounded_full().bg(co.accent)))
}

/// A check mark (`.rc-check`): 17px box, accent fill + tick when on.
fn check_mark(on: bool, _disabled: bool, co: &Co) -> impl IntoElement {
    div()
        .size(px(17.))
        .flex_shrink_0()
        .rounded(px(5.))
        .border_2()
        .border_color(if on { co.accent } else { co.faint })
        .flex()
        .items_center()
        .justify_center()
        .when(on, |d| d.bg(co.accent))
        .when(on, |d| d.child(Icon::new(PmIcon::Check).size(px(12.)).text_color(gpui::white())))
}

/// A `.rc-count` badge: mono, bordered pill.
fn count_badge(n: u64, co: &Co) -> impl IntoElement {
    div()
        .font_family("Consolas")
        .text_size(px(10.5))
        .text_color(co.muted)
        .bg(co.bg2)
        .border_1()
        .border_color(co.border)
        .rounded(px(5.))
        .px(px(7.))
        .py(px(1.))
        .child(group_thousands(n))
}

/// A `.save-group-label`: 13px semibold.
fn group_label(text: String, co: &Co) -> Div {
    div().mb(px(8.)).text_size(px(13.)).font_semibold().text_color(co.fg).child(text)
}

/// Formats a number with thousands separators (cf. design `toLocaleString`).
fn group_thousands(n: u64) -> SharedString {
    let s = n.to_string();
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(*b as char);
    }
    out.into()
}
