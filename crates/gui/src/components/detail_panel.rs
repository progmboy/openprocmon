//! The detail panel — a docked view entity (beside the event table) holding the
//! selected [`EventDetail`] + active tab. Layout/sizes/colors follow
//! `docs/design/gui-design-v2` (`detail.jsx` / `panels.css`): a header with a
//! close button, underline tabs (Event / Process / Stack), and per-tab content
//! built from kv-groups, field-boxes, codeblocks, tags and a stack table.

use gpui::{
    div, prelude::FluentBuilder, px, transparent_black, AppContext, Context, Entity, Hsla,
    InteractiveElement, IntoElement, ParentElement, ScrollHandle, SharedString,
    StatefulInteractiveElement, Styled, WeakEntity, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants},
    h_flex,
    input::{Input, InputEvent, InputState},
    scroll::ScrollableElement,
    tooltip::Tooltip,
    v_flex, ActiveTheme, Icon, Sizable, StyledExt,
};
use rust_i18n::t;

use crate::app::AppView;
use crate::icons::PmIcon;
use crate::model::domain::{EventDetail, ProcessNode};
use crate::theme::{palette, ProcmonPalette};

/// Resolved colors for the panel, derived once per render from theme + palette.
#[derive(Clone, Copy)]
struct Co {
    fg: Hsla,
    text2: Hsla,
    muted: Hsla,
    faint: Hsla,
    border: Hsla,
    /// The recessed field/codeblock background (design `--bg-2`).
    bg2: Hsla,
    /// The table/section header background (design `--panel`).
    head_bg: Hsla,
    accent: Hsla,
    row_hover: Hsla,
    /// Render PID/TID in hex (Settings ▸ Display Format).
    hex_id: bool,
    pal: ProcmonPalette,
}

/// Formats a PID/TID, in hex when enabled.
fn fmt_id(id: u32, hex: bool) -> String {
    if hex {
        format!("0x{:x}", id)
    } else {
        id.to_string()
    }
}

pub(crate) struct DetailView {
    app: WeakEntity<AppView>,
    detail: Option<EventDetail>,
    tab: usize,
    mod_filter: Entity<InputState>,
    /// Horizontal scroll handle for the stack table (wheel-restricted).
    stack_scroll: ScrollHandle,
    /// Bumped on every `set_detail`; an async symbol-resolution result is applied
    /// only if its captured generation still matches (guards against the user
    /// switching rows before resolution finishes).
    symbol_gen: u64,
}

impl DetailView {
    pub(crate) fn new(
        app: WeakEntity<AppView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mod_filter = cx
            .new(|cx| InputState::new(window, cx).placeholder(t!("dt.filter_modules").to_string()));
        // Re-render (re-filter the module list) as the filter text changes.
        cx.subscribe(&mod_filter, |_, _, ev: &InputEvent, cx| {
            if matches!(ev, InputEvent::Change) {
                cx.notify();
            }
        })
        .detach();

        Self {
            app,
            detail: None,
            tab: 0,
            mod_filter,
            stack_scroll: ScrollHandle::new(),
            symbol_gen: 0,
        }
    }

    pub(crate) fn set_detail(&mut self, detail: EventDetail, cx: &mut Context<Self>) {
        self.detail = Some(detail);
        // Invalidate any in-flight symbol resolution for the previous row.
        self.symbol_gen = self.symbol_gen.wrapping_add(1);
        cx.notify();
    }

    /// The current resolution generation (captured by the async symbol task).
    pub(crate) fn symbol_gen(&self) -> u64 {
        self.symbol_gen
    }

    /// Applies resolved call-stack symbols (frame index → `module!symbol+off`) to the
    /// current detail, ignoring stale results from a previously selected row.
    pub(crate) fn apply_symbols(
        &mut self,
        generation: u64,
        symbols: Vec<(usize, String)>,
        cx: &mut Context<Self>,
    ) {
        if generation != self.symbol_gen {
            return;
        }
        let Some(detail) = self.detail.as_mut() else {
            return;
        };
        for (i, sym) in symbols {
            if let Some(row) = detail.stack.get_mut(i) {
                row.location = sym.into();
            }
        }
        cx.notify();
    }
}

impl gpui::Render for DetailView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let hex_id = self
            .app
            .upgrade()
            .map(|a| a.read(cx).state.read(cx).config.hex_thread_proc_id)
            .unwrap_or(false);
        let co = Co {
            fg: cx.theme().foreground,
            text2: cx.theme().foreground.opacity(0.72),
            muted: cx.theme().muted_foreground,
            faint: cx.theme().muted_foreground.opacity(0.7),
            border: cx.theme().border,
            bg2: cx.theme().background,
            head_bg: cx.theme().table_head,
            accent: palette(cx).row_sel_bar,
            row_hover: cx.theme().list_hover,
            hex_id,
            pal: palette(cx),
        };

        let root = v_flex().size_full().bg(cx.theme().secondary);

        let Some(d) = self.detail.as_ref() else {
            return root
                .items_center()
                .justify_center()
                .gap_3()
                .p(px(30.))
                .text_color(co.faint)
                .child(Icon::new(PmIcon::FileText).size(px(46.)))
                .child(
                    div()
                        .text_color(co.text2)
                        .text_base()
                        .font_semibold()
                        .child(t!("dt.no_event").to_string()),
                )
                .into_any_element();
        };

        let cat = d.category.color(&co.pal);
        let tab = self.tab;
        let frames = d.stack.len();
        let filter = self.mod_filter.read(cx).value().to_string();

        let body = match tab {
            1 => process_tab(d, &co, &filter, &self.mod_filter),
            2 => stack_tab(d, &co, &self.stack_scroll),
            _ => event_tab(d, &co),
        };

        root
            // Header (.detail-head).
            .child(
                h_flex()
                    .items_center()
                    .gap_3()
                    .px(px(16.))
                    .py(px(14.))
                    .border_b_1()
                    .border_color(co.border)
                    .child(crate::components::app_icon(
                        d.process.icon.as_ref(),
                        &d.process.name,
                        cat,
                        20.,
                    ))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w(px(0.))
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_color(co.fg)
                                            .text_base()
                                            .font_semibold()
                                            .child(d.process.name.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_color(co.muted)
                                            .text_sm()
                                            .child(format!("PID {}", fmt_id(d.pid, co.hex_id))),
                                    ),
                            )
                            .child(
                                div()
                                    .text_color(co.muted)
                                    .text_sm()
                                    .truncate()
                                    .child(format!("{} · {}", d.operation, d.time)),
                            ),
                    )
                    .child(
                        Button::new("detail-close")
                            .ghost()
                            .icon(PmIcon::X)
                            .tooltip(t!("dt.close").to_string())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.app.update(cx, |v, cx| v.close_detail(cx)).ok();
                            })),
                    ),
            )
            // Underline tabs (.tabs) — custom for exact colors/width/underline.
            .child(
                h_flex()
                    .gap(px(2.))
                    .px(px(8.))
                    .pt(px(6.))
                    .border_b_1()
                    .border_color(co.border)
                    .bg(cx.theme().secondary)
                    .child(tab_btn(
                        0,
                        tab,
                        PmIcon::Info,
                        t!("dt.tab_event").to_string(),
                        None,
                        &co,
                        cx,
                    ))
                    .child(tab_btn(
                        1,
                        tab,
                        PmIcon::Cpu,
                        t!("dt.tab_process").to_string(),
                        None,
                        &co,
                        cx,
                    ))
                    .child(tab_btn(
                        2,
                        tab,
                        PmIcon::Layers,
                        t!("dt.tab_stack").to_string(),
                        Some(frames),
                        &co,
                        cx,
                    )),
            )
            // Content (.detail-content) — scrolls.
            .child(
                // master's `Scrollable` only copies the wrapped element's *size*
                // to its outer wrapper (flex_1/min_h are dropped), so the flex
                // that bounds the scroll region must live on a parent div here —
                // otherwise the wrapper is size_full, overflows, and never scrolls.
                div().flex_1().min_h(px(0.)).child(
                    div()
                        .id("detail-body")
                        .size_full()
                        .py(px(6.))
                        .child(body)
                        .overflow_y_scrollbar(),
                ),
            )
            .into_any_element()
    }
}

// ---------------------------------------------------------------------------
// Building blocks (mirroring panels.css)
// ---------------------------------------------------------------------------

/// `.kv-group`: padded section with an optional title (with a leading icon, per
/// the design's `.kv-title`) + trailing rule.
fn group(
    icon: Option<PmIcon>,
    title: Option<&str>,
    co: &Co,
    body: impl IntoElement,
) -> impl IntoElement {
    v_flex()
        .px(px(16.))
        .pt(px(8.))
        .pb(px(14.))
        .when_some(title.map(|t| t.to_string()), |this, title| {
            this.child(
                h_flex()
                    .items_center()
                    .gap(px(8.))
                    .mt_2()
                    .mb_3()
                    .when_some(icon, |this, ic| {
                        this.child(Icon::new(ic).size(px(13.)).text_color(co.faint))
                    })
                    .child(
                        div()
                            .text_color(co.faint)
                            .text_xs()
                            .font_semibold()
                            .child(title.to_uppercase()),
                    )
                    .child(div().flex_1().h(px(1.)).bg(co.border)),
            )
        })
        .child(body)
}

/// `.kv`: a key (110px, left, muted) + right-aligned value (mono, colored).
fn kv(k: &str, v: impl Into<SharedString>, v_color: Hsla, co: &Co) -> impl IntoElement {
    h_flex()
        .min_h(px(26.))
        .py(px(3.))
        .items_center()
        .justify_between()
        .gap_4()
        .child(
            div()
                .w(px(110.))
                .flex_shrink_0()
                .text_color(co.muted)
                .text_sm()
                .child(k.to_string()),
        )
        .child(
            div()
                .flex_1()
                .text_right()
                .text_color(v_color)
                .text_sm()
                .child(v.into()),
        )
}

/// `.evt-field`: a small label above a `.field-box`.
fn field(label: &str, co: &Co, content: impl IntoElement) -> impl IntoElement {
    v_flex()
        .px(px(16.))
        .pt(px(10.))
        .pb(px(0.5))
        .child(
            div()
                .text_color(co.muted)
                .text_sm()
                .mb_2()
                .child(label.to_string()),
        )
        .child(
            div()
                .w_full()
                .bg(co.bg2)
                .border_1()
                .border_color(co.border)
                .rounded(px(8.))
                .px(px(15.))
                .py(px(12.))
                .child(content),
        )
}

/// `.codeblock`: recessed monospace block.
fn codeblock(text: impl Into<SharedString>, color: Hsla, co: &Co) -> impl IntoElement {
    div()
        .w_full()
        .bg(co.bg2)
        .border_1()
        .border_color(co.border)
        .rounded(px(6.))
        .px(px(11.))
        .py(px(9.))
        .text_color(color)
        .text_sm()
        .child(text.into())
}

/// `.kv.block`: uppercase key + a code block below it.
fn kv_block(k: &str, co: &Co, block: impl IntoElement) -> impl IntoElement {
    v_flex()
        .gap_1p5()
        .mb_2()
        .child(div().text_color(co.muted).text_xs().child(k.to_uppercase()))
        .child(block)
}

/// `.tag`: small colored pill (fg over an 18%-tinted fill).
fn tag(text: impl Into<SharedString>, color: Hsla) -> impl IntoElement {
    div()
        .px_2()
        .py(px(2.))
        .rounded_full()
        .text_xs()
        .font_semibold()
        .text_color(color)
        .bg(color.opacity(0.18))
        .child(text.into())
}

/// A custom underline tab (`.tab`): icon + label (+ optional count badge),
/// muted by default, accent text + accent underline when active.
fn tab_btn(
    idx: usize,
    active_idx: usize,
    icon: PmIcon,
    label: String,
    badge: Option<usize>,
    co: &Co,
    cx: &mut Context<DetailView>,
) -> impl IntoElement {
    let active = idx == active_idx;
    let color = if active { co.accent } else { co.muted };
    div()
        .id(("dtab", idx))
        .flex()
        .items_center()
        .gap(px(7.))
        .px(px(14.))
        .py(px(8.))
        .border_b_2()
        .border_color(if active {
            co.accent
        } else {
            transparent_black()
        })
        .text_color(color)
        .text_sm()
        .cursor_pointer()
        .child(Icon::new(icon).size(px(14.)).text_color(color))
        .child(label)
        .when_some(badge, |this, n| {
            this.child(
                div()
                    .px_1p5()
                    .rounded_full()
                    .text_xs()
                    .bg(if active {
                        co.accent.opacity(0.16)
                    } else {
                        co.bg2
                    })
                    .text_color(color)
                    .child(n.to_string()),
            )
        })
        .on_click(cx.listener(move |this, _, _, cx| {
            this.tab = idx;
            cx.notify();
        }))
}

// ---------------------------------------------------------------------------
// Tabs
// ---------------------------------------------------------------------------

fn event_tab(d: &EventDetail, co: &Co) -> gpui::AnyElement {
    let cat = d.category.color(&co.pal);
    let res = d.result_kind.color(&co.pal);

    let mut col = v_flex()
        // Category header (.evt-cat-head).
        .child(
            h_flex()
                .items_center()
                .gap_2()
                .px(px(16.))
                .pt(px(14.))
                .pb(px(0.5))
                .child(Icon::new(category_icon(d)).size(px(18.)).text_color(cat))
                .child(
                    div()
                        .text_color(cat)
                        .text_lg()
                        .font_semibold()
                        .child(d.category.label()),
                ),
        )
        // Operation field.
        .child(field(
            &t!("dt.operation"),
            co,
            div().text_color(cat).text_lg().child(d.operation.clone()),
        ))
        // Time group.
        .child(group(
            Some(PmIcon::Clock),
            Some(&t!("dt.time")),
            co,
            v_flex()
                .child(kv(&t!("dt.date"), d.date.clone(), co.text2, co))
                .child(kv(&t!("dt.timestamp"), d.time.clone(), co.text2, co))
                .child(kv(
                    &t!("dt.duration"),
                    d.duration
                        .clone()
                        .map(|s| format!("{s} s"))
                        .unwrap_or_else(|| "—".into()),
                    co.pal.res_success,
                    co,
                )),
        ))
        // Process info group.
        .child(group(
            Some(PmIcon::Cpu),
            Some(&t!("dt.process")),
            co,
            v_flex()
                .child(kv(&t!("dt.pid"), fmt_id(d.pid, co.hex_id), co.pal.pid, co))
                .child(kv(&t!("dt.tid"), fmt_id(d.tid, co.hex_id), co.pal.pid, co)),
        ));

    if !d.path.is_empty() {
        col = col.child(group(
            Some(PmIcon::Open),
            Some(&t!("dt.path")),
            co,
            codeblock(d.path.clone(), co.pal.path, co),
        ));
    }

    // Result field.
    col = col.child(field(
        &t!("dt.result"),
        co,
        div()
            .text_color(res)
            .text_lg()
            .font_semibold()
            .child(d.result.clone()),
    ));

    // Target file group (file events with metadata).
    if let (Some(version), Some(company)) = (d.target_version.clone(), d.target_company.clone()) {
        let signed = d.signed.unwrap_or(false);
        col = col.child(group(
            None,
            Some(&t!("dt.target_file")),
            co,
            v_flex()
                .child(kv(&t!("dt.version"), version, co.text2, co))
                .child(kv(&t!("dt.company"), company, co.text2, co))
                .child(
                    h_flex()
                        .min_h(px(26.))
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .w(px(110.))
                                .text_color(co.muted)
                                .text_sm()
                                .child(t!("dt.signed").to_string()),
                        )
                        .child(if signed {
                            tag(t!("dt.signed").to_string(), co.pal.res_success).into_any_element()
                        } else {
                            tag(t!("dt.unsigned").to_string(), co.muted).into_any_element()
                        }),
                ),
        ));
    }

    // Other details (read-only multi-line block).
    col = col.child(field(
        &t!("dt.other_details"),
        co,
        v_flex().gap_0p5().children(
            d.other_details
                .split('\n')
                .map(|line| div().text_color(co.text2).text_sm().child(line.to_string())),
        ),
    ));

    col.into_any_element()
}

fn process_tab(
    d: &EventDetail,
    co: &Co,
    filter: &str,
    mod_input: &Entity<InputState>,
) -> gpui::AnyElement {
    let p: &ProcessNode = &d.process;
    let cat = d.category.color(&co.pal);
    let integrity_color = integrity_color(p.integrity.as_ref(), co);
    let q = filter.to_ascii_lowercase();

    v_flex()
        // Header group: icon + name/company/version + status/integrity tags.
        .child(group(
            None,
            None,
            co,
            v_flex()
                .child(
                    h_flex()
                        .gap_3()
                        .items_center()
                        .pb(px(8.))
                        .child(crate::components::app_icon(
                            p.icon.as_ref(),
                            &p.name,
                            cat,
                            40.,
                        ))
                        .child(
                            v_flex()
                                .min_w(px(0.))
                                .gap_0p5()
                                .child(
                                    div()
                                        .text_color(co.fg)
                                        .text_lg()
                                        .font_semibold()
                                        .child(p.name.clone()),
                                )
                                .child(
                                    div()
                                        .text_color(co.muted)
                                        .text_sm()
                                        .child(p.company.clone()),
                                )
                                .child(
                                    h_flex()
                                        .gap_1()
                                        .child(
                                            div()
                                                .text_color(co.muted)
                                                .text_sm()
                                                .child(t!("dt.version").to_string()),
                                        )
                                        .child(
                                            div()
                                                .text_color(co.text2)
                                                .text_sm()
                                                .child(p.version.clone()),
                                        ),
                                )
                                .child(
                                    h_flex()
                                        .gap_1p5()
                                        .mt_1()
                                        .child(if p.running {
                                            tag(t!("dt.running").to_string(), co.pal.res_success)
                                                .into_any_element()
                                        } else {
                                            tag(t!("dt.exited").to_string(), co.muted)
                                                .into_any_element()
                                        })
                                        .child(tag(p.integrity.clone(), integrity_color)),
                                ),
                        ),
                )
                .child(kv(&t!("dt.pid"), fmt_id(p.pid, co.hex_id), co.text2, co))
                .child(kv(
                    &t!("dt.architecture"),
                    format!("{}-bit", p.arch),
                    co.text2,
                    co,
                ))
                .child(kv(
                    &t!("dt.parent_pid"),
                    p.parent_pid.to_string(),
                    co.text2,
                    co,
                ))
                .child(kv(
                    &t!("dt.virtualized"),
                    if p.virtualized {
                        t!("dt.yes")
                    } else {
                        t!("dt.no")
                    }
                    .to_string(),
                    co.text2,
                    co,
                ))
                .child(kv(
                    &t!("dt.session"),
                    p.session_id.to_string(),
                    co.text2,
                    co,
                ))
                .child(kv(&t!("dt.integrity"), p.integrity.clone(), co.text2, co))
                .child(kv(&t!("dt.user"), p.user.clone(), co.text2, co))
                .child(kv(&t!("dt.start_time"), p.start_time.clone(), co.text2, co)),
        ))
        // Path + command line.
        .child(group(
            None,
            None,
            co,
            v_flex()
                .child(kv_block(
                    &t!("dt.path"),
                    co,
                    codeblock(p.image_path.clone(), co.pal.path, co),
                ))
                .child(kv_block(
                    &t!("dt.command_line"),
                    co,
                    codeblock(p.command_line.clone(), co.text2, co),
                )),
        ))
        // Modules (with a filter box, design `.mod-search` + `.mod-list`).
        .child(group(
            None,
            Some(&format!("{} ({})", t!("dt.modules"), d.modules.len())),
            co,
            v_flex()
                .child(
                    // `.mod-search`: a search input with a leading icon. Using the
                    // input's own `appearance` (default on) gives the border + the
                    // accent focus highlight (theme.ring) for free.
                    div().w_full().mb_2().child(
                        Input::new(mod_input)
                            .small()
                            .w_full()
                            .prefix(Icon::new(PmIcon::Search).size(px(13.)).text_color(co.muted))
                            .cleanable(true)
                            .map(|mut i| {
                                // Design `.mod-search { height: 30px }`. Single-line
                                // `Input::h()` is ignored, so force the Styled height.
                                i.style().size.height = Some(px(30.).into());
                                i
                            }),
                    ),
                )
                .child(
                    v_flex().gap_0p5().children(
                        d.modules
                            .iter()
                            .filter(|m| {
                                q.is_empty()
                                    || m.name.to_ascii_lowercase().contains(&q)
                                    || m.path.to_ascii_lowercase().contains(&q)
                            })
                            .enumerate()
                            .map(|(i, m)| {
                                // `.mod-row` with hover highlight.
                                div()
                                    .id(("mod", i))
                                    .px(px(10.))
                                    .py(px(7.))
                                    .rounded(px(6.))
                                    .hover(|s| s.bg(co.row_hover))
                                    .child(
                                        h_flex()
                                            .justify_between()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .text_color(co.fg)
                                                    .text_sm()
                                                    .child(m.name.clone()),
                                            )
                                            .child(
                                                div()
                                                    .text_color(co.muted)
                                                    .text_xs()
                                                    .child(format!("0x{:x}", m.base)),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_color(co.faint)
                                            .text_xs()
                                            .truncate()
                                            .child(m.path.clone()),
                                    )
                            }),
                    ),
                ),
        ))
        .into_any_element()
}

fn stack_tab(d: &EventDetail, co: &Co, scroll: &ScrollHandle) -> gpui::AnyElement {
    // `.stack-note`: "Operation <op> call stack · N frames", op emphasized.
    let note = h_flex()
        .items_center()
        .gap_2()
        .px(px(16.))
        .py(px(9.))
        .text_color(co.muted)
        .text_xs()
        .child(Icon::new(PmIcon::Layers).size(px(14.)))
        .child(t!("dt.cs_pre").to_string())
        .child(
            div()
                .text_color(co.fg)
                .font_semibold()
                .child(d.operation.clone()),
        )
        .child(t!("dt.cs_mid").to_string())
        .child(d.stack.len().to_string())
        .child(t!("dt.cs_suf").to_string());

    // Fixed-width columns so the table can scroll horizontally in the panel.
    const W_FRAME: f32 = 56.0;
    const W_MOD: f32 = 130.0;
    const W_LOC: f32 = 220.0;
    const W_ADDR: f32 = 140.0;
    const W_PATH: f32 = 200.0;
    let total_w = W_FRAME + W_MOD + W_LOC + W_ADDR + W_PATH;

    let th = |label: &str, w: f32| {
        div()
            .w(px(w))
            .flex_shrink_0()
            .px(px(10.))
            .py(px(7.))
            .text_color(co.muted)
            .text_xs()
            .font_semibold()
            .child(label.to_string())
    };
    let header = h_flex()
        .w(px(total_w))
        // Design `.stack-table th { background: var(--panel) }`.
        .bg(co.head_bg)
        .border_b_1()
        .border_color(co.border)
        .child(th("Frame", W_FRAME))
        .child(th("Module", W_MOD))
        .child(th("Location", W_LOC))
        .child(th("Address", W_ADDR))
        .child(th("Path", W_PATH));

    let td = |w: f32, color: Hsla, text: SharedString, bold: bool| {
        let mut c = div()
            .w(px(w))
            .flex_shrink_0()
            .px(px(10.))
            .py(px(6.))
            .text_color(color)
            .text_xs()
            .truncate()
            .child(text);
        if bold {
            c = c.font_semibold();
        }
        c
    };
    let rows = v_flex().children(d.stack.iter().enumerate().map(|(i, f)| {
        let kind_color = f.kind.color(&co.pal);
        let mod_color = if f.kind == crate::model::domain::FrameKind::Kernel {
            co.pal.op_registry
        } else {
            co.pal.op_file
        };
        h_flex()
            .id(("stack", i))
            .w(px(total_w))
            .items_center()
            .bg(kind_color.opacity(0.07))
            .border_b_1()
            .border_color(co.border.opacity(0.5))
            .hover(move |s| s.bg(kind_color.opacity(0.16)))
            .child(td(W_FRAME, kind_color, f.frame.to_string().into(), true))
            .child(td(W_MOD, mod_color, f.module.clone(), true))
            .child(td(W_LOC, co.text2, f.location.clone(), false))
            .child(td(
                W_ADDR,
                co.pal.res_success.opacity(0.82),
                format!("0x{:x}", f.address).into(),
                false,
            ))
            .child({
                // Path cell: truncated like the rest, with the full path as a
                // tooltip (same pattern as the event table's Path column).
                let path = f.path.clone();
                td(W_PATH, co.faint, path.clone(), false)
                    .id(("stack-path", i))
                    .when(!path.is_empty(), |c| {
                        c.tooltip(move |window, cx| Tooltip::new(path.clone()).build(window, cx))
                    })
            })
    }));

    // Horizontal-scroll the wide table without the vertical wheel hijacking it
    // (see `components::h_scroll_area`). With no frames the scroll area would show
    // just an empty header + a stray scrollbar, so render a centered empty state
    // instead (events without a captured stack — e.g. PML logs saved without one).
    let table: gpui::AnyElement = if d.stack.is_empty() {
        v_flex()
            .items_center()
            .justify_center()
            .gap_2()
            .py(px(40.))
            .text_color(co.faint)
            .child(Icon::new(PmIcon::Layers).size(px(34.)))
            .child(
                div()
                    .text_color(co.text2)
                    .text_sm()
                    .child(t!("dt.no_stack").to_string()),
            )
            .into_any_element()
    } else {
        crate::components::h_scroll_area(
            "stack",
            scroll,
            v_flex().w(px(total_w)).child(header).child(rows),
        )
        .into_any_element()
    };

    // `.stack-legend`: a recessed box with a "Note:" title + K/U rows stacked.
    let lg_badge = |t: &str, c: Hsla| {
        div()
            .size(px(18.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(5.))
            .text_xs()
            .font_semibold()
            .text_color(c)
            .bg(c.opacity(0.18))
            .child(t.to_string())
    };
    let legend = v_flex()
        .gap(px(7.))
        .mx(px(16.))
        .my(px(14.))
        .px(px(14.))
        .py(px(11.))
        .rounded(px(7.))
        .border_1()
        .border_color(co.border)
        .bg(co.bg2)
        .text_sm()
        .text_color(co.text2)
        .child(
            div()
                .text_color(co.fg)
                .font_semibold()
                .child(t!("dt.note").to_string()),
        )
        .child(
            h_flex()
                .gap_1()
                .items_center()
                .child(lg_badge("K", co.pal.frame_kernel))
                .child(format!(" = {}", t!("dt.kernel_mode"))),
        )
        .child(
            h_flex()
                .gap_1()
                .items_center()
                .child(lg_badge("U", co.pal.frame_user))
                .child(format!(" = {}", t!("dt.user_mode"))),
        );

    v_flex()
        .child(note)
        .child(table)
        .child(legend)
        .into_any_element()
}

fn category_icon(d: &EventDetail) -> PmIcon {
    use crate::model::domain::EventCategory::*;
    match d.category {
        Registry => PmIcon::Registry,
        File => PmIcon::Filesys,
        Network => PmIcon::Network,
        Process => PmIcon::ProcThread,
        Profiling => PmIcon::Perf,
        Other => PmIcon::Info,
    }
}

fn integrity_color(integrity: &str, co: &Co) -> Hsla {
    match integrity {
        "System" | "Protected" => co.pal.integrity_system,
        "High" => co.pal.integrity_high,
        "Medium" | "Medium+" => co.pal.integrity_medium,
        "Low" | "Untrusted" => co.pal.integrity_low,
        _ => co.muted,
    }
}
