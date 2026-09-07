//! The unified Settings dialog (design `settings.jsx`) — left category nav + right
//! scrollable panel, wrapped in the shared `FormDialog`. Replaces the old separate
//! Configure-Symbols / History-Depth dialogs.
//!
//! It's a view entity holding a `draft` of the config (+ theme/locale + the text
//! `InputState`s). Opening calls `load()` to seed the draft from `AppState`; Apply
//! commits it back via `AppView::apply_settings`. Theme/locale/highlight-color/hex
//! take effect immediately; symbols/history/profiling/boot are stored for the SDK.

use gpui_kit::component::{
    button::{Button, ButtonVariants},
    h_flex,
    input::{Input, InputState},
    scroll::ScrollableElement,
    slider::{Slider, SliderEvent, SliderState},
    v_flex, ActiveTheme, Icon, Sizable, StyledExt, ThemeMode, WindowExt,
};
use gpui_kit::{
    black, div, img, prelude::FluentBuilder, px, transparent_black, white, App, AppContext,
    Context, Div, Entity, InteractiveElement, IntoElement, ObjectFit, ParentElement, Stateful,
    StatefulInteractiveElement, Styled, StyledImage, WeakEntity, Window,
};
use rust_i18n::t;

use crate::app::AppView;
use crate::components::input_h;
use crate::icons::PmIcon;
use crate::model::config::{
    AppConfig, HighlightColor, ProfilingInterval, BG_OPACITY_MAX, BG_OPACITY_MIN,
};
use crate::theme::palette;

/// Category nav entries (icon + i18n key), in order.
const CATS: [(PmIcon, &str); 6] = [
    (PmIcon::Palette, "set.appearance"),
    (PmIcon::Layers, "set.symbols"),
    (PmIcon::Clock, "set.history"),
    (PmIcon::Perf, "set.profiling"),
    (PmIcon::Power, "set.boot"),
    (PmIcon::Hash, "set.display"),
];

pub(crate) struct SettingsDialog {
    app: WeakEntity<AppView>,
    selected: usize,
    draft: AppConfig,
    theme: ThemeMode,
    lang_zh: bool,
    sym: Entity<InputState>,
    dbg: Entity<InputState>,
    mb: Entity<InputState>,
    min: Entity<InputState>,
    /// Background image opacity. The slider owns the live value; its `Change`
    /// event mirrors it into `draft.bg_opacity`, which is what Apply commits.
    bg_opacity: Entity<SliderState>,
}

impl SettingsDialog {
    pub(crate) fn new(
        app: WeakEntity<AppView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let sym = cx.new(|cx| InputState::new(window, cx));
        let dbg = cx.new(|cx| InputState::new(window, cx));
        let mb = cx.new(|cx| InputState::new(window, cx));
        let min = cx.new(|cx| InputState::new(window, cx));
        let bg_opacity = cx.new(|_| {
            SliderState::new()
                .min(f32::from(BG_OPACITY_MIN))
                .max(f32::from(BG_OPACITY_MAX))
                .step(5.)
                .default_value(f32::from(AppConfig::default().bg_opacity))
        });
        cx.subscribe(&bg_opacity, |this: &mut Self, _, ev: &SliderEvent, cx| {
            if let SliderEvent::Change(value) = ev {
                this.draft.bg_opacity = value.start().round() as u8;
                cx.notify();
            }
        })
        .detach();
        Self {
            app,
            selected: 0,
            draft: AppConfig::default(),
            theme: ThemeMode::Dark,
            lang_zh: true,
            sym,
            dbg,
            mb,
            min,
            bg_opacity,
        }
    }

    /// Picks a background image. The native dialog runs a blocking modal loop, so
    /// it goes on its own thread (same reason as `SaveDialog::browse`) and the
    /// result is applied back to the draft on the UI loop.
    fn browse_background(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (tx, rx) = crossbeam_channel::bounded::<Option<std::path::PathBuf>>(1);
        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new()
                .add_filter(
                    "Images",
                    &["png", "jpg", "jpeg", "bmp", "gif", "webp", "svg"],
                )
                .pick_file();
            let _ = tx.send(picked);
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
            if let Some(path) = picked {
                let _ = this.update(cx, |this, cx| {
                    this.draft.bg_image = path.to_string_lossy().to_string();
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// Seeds the draft when opening. The config/theme are passed in (read by the
    /// caller) rather than read here through `app` — `load` runs inside `AppView`'s
    /// own borrow, so re-reading AppView via the weak handle would panic.
    pub(crate) fn load(
        &mut self,
        config: AppConfig,
        theme: ThemeMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.draft = config;
        self.theme = theme;
        self.lang_zh = rust_i18n::locale().starts_with("zh");
        let (sym, dbg) = (
            self.draft.symbols_path.clone(),
            self.draft.dbghelp_path.clone(),
        );
        let (mb, min) = (
            self.draft.history_mb.to_string(),
            self.draft.history_min.to_string(),
        );
        self.sym.update(cx, |s, cx| s.set_value(sym, window, cx));
        self.dbg.update(cx, |s, cx| s.set_value(dbg, window, cx));
        self.mb.update(cx, |s, cx| s.set_value(mb, window, cx));
        self.min.update(cx, |s, cx| s.set_value(min, window, cx));
        let opacity = f32::from(self.draft.bg_opacity);
        self.bg_opacity
            .update(cx, |s, cx| s.set_value(opacity, window, cx));
        cx.notify();
    }

    fn apply(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut cfg = self.draft.clone();
        cfg.symbols_path = self.sym.read(cx).value().to_string();
        cfg.dbghelp_path = self.dbg.read(cx).value().to_string();
        cfg.history_mb = parse_min1(&self.mb.read(cx).value(), cfg.history_mb);
        cfg.history_min = parse_min1(&self.min.read(cx).value(), cfg.history_min);
        let (theme, zh) = (self.theme, self.lang_zh);
        if let Some(app) = self.app.upgrade() {
            app.update(cx, |view, cx| {
                view.apply_settings(cfg, theme, zh, window, cx)
            });
        }
        window.close_dialog(cx);
    }

    /// Footer: Cancel + Apply.
    pub(crate) fn footer(dialog: &Entity<Self>) -> impl IntoElement {
        let ok = dialog.clone();
        h_flex()
            .w_full()
            .items_center()
            .justify_end()
            .gap(px(8.))
            .child(
                Button::new("set-cancel")
                    .h(px(34.))
                    .label(t!("dlg.cancel").to_string())
                    .on_click(|_, window, cx| window.close_dialog(cx)),
            )
            .child(
                Button::new("set-apply")
                    .primary()
                    .h(px(34.))
                    .label(t!("dlg.apply").to_string())
                    .on_click(move |_, window, cx| ok.update(cx, |d, cx| d.apply(window, cx))),
            )
    }
}

impl gpui_kit::Render for SettingsDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let co = Co::new(cx);

        // Left nav (design `.settings-nav`).
        let nav = v_flex()
            .w(px(188.))
            .h_full()
            .flex_shrink_0()
            .gap(px(3.))
            .px(px(10.))
            .py(px(12.))
            .bg(co.panel)
            .border_r_1()
            .border_color(co.border)
            .children(CATS.iter().enumerate().map(|(ix, (icon, key))| {
                let active = self.selected == ix;
                div()
                    .id(ix)
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .px(px(11.))
                    .py(px(9.))
                    .rounded(px(8.))
                    .cursor_pointer()
                    .text_size(px(12.5))
                    .map(|d| {
                        if active {
                            d.bg(co.accent_soft).text_color(co.accent).font_semibold()
                        } else {
                            d.text_color(co.text2).hover(|s| s.bg(co.hover))
                        }
                    })
                    .child(Icon::new(*icon).size(px(16.)).text_color(if active {
                        co.accent
                    } else {
                        co.muted
                    }))
                    .child(t!(*key).to_string())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.selected = ix;
                        cx.notify();
                    }))
            }));

        // Right content (design `.settings-content`), scrollable.
        let panel = match self.selected {
            0 => self.panel_appearance(&co, cx).into_any_element(),
            1 => self.panel_symbols(&co).into_any_element(),
            2 => self.panel_history(&co, cx).into_any_element(),
            3 => self.panel_profiling(&co, cx).into_any_element(),
            4 => self.panel_boot(&co, cx).into_any_element(),
            _ => self.panel_display(&co, cx).into_any_element(),
        };
        let content = div().flex_1().min_w(px(0.)).h_full().child(
            div()
                .id("set-content")
                .size_full()
                .px(px(26.))
                .py(px(22.))
                .child(panel)
                .overflow_y_scrollbar(),
        );

        // design `.settings-dialog` body is 760×(560 - head - foot) ≈ 452 tall.
        h_flex().w_full().h(px(452.)).child(nav).child(content)
    }
}

impl SettingsDialog {
    /// Appearance ▸ Background Image (design `.bg-pick`): a preview thumbnail, the
    /// file name, and Browse / Remove.
    ///
    /// The draft holds the path, so the thumbnail is whatever the user just picked
    /// rather than what is currently applied — Apply is what commits it.
    fn bg_picker(&self, co: &Co, cx: &mut Context<Self>) -> impl IntoElement {
        let path = self.draft.bg_image.trim();
        let picked = (!path.is_empty()).then(|| std::path::PathBuf::from(path));
        let name = picked
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| t!("set.bg_none").to_string());
        h_flex()
            .items_center()
            .gap(px(13.))
            .child(
                div()
                    .w(px(78.))
                    .h(px(50.))
                    .flex_shrink_0()
                    .rounded(px(8.))
                    .overflow_hidden()
                    .border_1()
                    .border_color(co.border)
                    .bg(co.panel)
                    .flex()
                    .items_center()
                    .justify_center()
                    .map(|thumb| match picked.clone() {
                        Some(path) => {
                            thumb.child(img(path).size_full().object_fit(ObjectFit::Cover))
                        }
                        None => thumb
                            .text_color(co.faint)
                            .child(Icon::new(PmIcon::Palette).size(px(18.))),
                    }),
            )
            .child(
                v_flex()
                    .min_w(px(0.))
                    .gap(px(8.))
                    .child(
                        div()
                            .max_w(px(300.))
                            .truncate()
                            .text_size(px(11.5))
                            .text_color(co.text2)
                            .child(name),
                    )
                    .child(
                        h_flex()
                            .gap(px(8.))
                            .child(
                                Button::new("bg-browse")
                                    .small()
                                    .label(t!("set.bg_browse").to_string())
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.browse_background(window, cx)
                                    })),
                            )
                            .when(picked.is_some(), |row| {
                                row.child(
                                    Button::new("bg-remove")
                                        .small()
                                        .label(t!("set.bg_remove").to_string())
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.draft.bg_image.clear();
                                            cx.notify();
                                        })),
                                )
                            }),
                    ),
            )
    }

    fn panel_appearance(&self, co: &Co, cx: &mut Context<Self>) -> impl IntoElement {
        let hl = self.draft.highlight_color;
        v_flex()
            .child(section_title(t!("set.appearance").to_string(), co).mb(px(8.)))
            .child(set_row(
                t!("set.theme").to_string(),
                Some(t!("set.theme_desc").to_string()),
                seg(co)
                    .child(
                        seg_btn(
                            "th-l",
                            t!("set.light").to_string(),
                            self.theme == ThemeMode::Light,
                            co,
                        )
                        .on_click(cx.listener(|t, _, _, cx| {
                            t.theme = ThemeMode::Light;
                            cx.notify();
                        })),
                    )
                    .child(
                        seg_btn(
                            "th-d",
                            t!("set.dark").to_string(),
                            self.theme == ThemeMode::Dark,
                            co,
                        )
                        .on_click(cx.listener(|t, _, _, cx| {
                            t.theme = ThemeMode::Dark;
                            cx.notify();
                        })),
                    ),
                co,
            ))
            .child(set_row(
                t!("set.lang").to_string(),
                Some(t!("set.lang_desc").to_string()),
                seg(co)
                    .child(
                        seg_btn("lg-zh", "中文".to_string(), self.lang_zh, co).on_click(
                            cx.listener(|t, _, _, cx| {
                                t.lang_zh = true;
                                cx.notify();
                            }),
                        ),
                    )
                    .child(
                        seg_btn("lg-en", "English".to_string(), !self.lang_zh, co).on_click(
                            cx.listener(|t, _, _, cx| {
                                t.lang_zh = false;
                                cx.notify();
                            }),
                        ),
                    ),
                co,
            ))
            .child(set_row_full(
                t!("set.hl_color").to_string(),
                Some(t!("set.hl_color_desc").to_string()),
                h_flex().gap(px(10.)).flex_wrap().children(
                    HighlightColor::ALL.iter().enumerate().map(|(i, c)| {
                        let c = *c;
                        swatch(i, c, hl == c, co).on_click(cx.listener(move |t, _, _, cx| {
                            t.draft.highlight_color = c;
                            cx.notify();
                        }))
                    }),
                ),
                co,
            ))
            .child(set_row_full(
                t!("set.bg_image").to_string(),
                Some(t!("set.bg_image_desc").to_string()),
                self.bg_picker(co, cx),
                co,
            ))
            // Opacity only matters once there is an image to fade (design hides it).
            .when(!self.draft.bg_image.is_empty(), |panel| {
                panel.child(set_row(
                    t!("set.bg_opacity").to_string(),
                    Some(t!("set.bg_opacity_desc").to_string()),
                    h_flex()
                        .items_center()
                        .gap(px(11.))
                        .child(div().w(px(130.)).child(Slider::new(&self.bg_opacity)))
                        .child(
                            div()
                                .w(px(38.))
                                .text_right()
                                .text_size(px(11.5))
                                .text_color(co.muted)
                                .child(format!("{}%", self.draft.bg_opacity)),
                        ),
                    co,
                ))
            })
            // Preview (design `.set-preview`): a highlighted row + a normal row.
            .child(
                v_flex()
                    .mt(px(16.))
                    .rounded(px(9.))
                    .overflow_hidden()
                    .border_1()
                    .border_color(co.border)
                    .bg(cx.theme().background)
                    .child(
                        div()
                            .px(px(13.))
                            .py(px(9.))
                            .text_size(px(11.5))
                            .font_family("Consolas")
                            .text_color(co.text2)
                            .bg(hl.hsla().opacity(0.22))
                            .child("chrome.exe   ReadFile   C:\\Windows\\System32\\…"),
                    )
                    .child(
                        div()
                            .px(px(13.))
                            .py(px(9.))
                            .text_size(px(11.5))
                            .font_family("Consolas")
                            .text_color(co.text2)
                            .border_t_1()
                            .border_color(co.border)
                            .child("svchost.exe  RegQueryKey  HKLM\\Software\\…"),
                    ),
            )
    }

    fn panel_symbols(&self, co: &Co) -> impl IntoElement {
        v_flex()
            .child(section_title(t!("set.sym_title").to_string(), co))
            .child(lead(t!("set.sym_lead").to_string(), co))
            .child(fld_label(t!("set.sym_path").to_string(), co))
            .child(fld(Input::new(&self.sym).w_full()))
            .child(fld_label(t!("set.dbghelp").to_string(), co))
            .child(fld(Input::new(&self.dbg).w_full()))
    }

    fn panel_history(&self, co: &Co, cx: &mut Context<Self>) -> impl IntoElement {
        let ring = self.draft.history_ring;
        v_flex()
            .child(section_title(t!("set.hist_title").to_string(), co).mb(px(8.)))
            .child(set_row(
                t!("set.ring").to_string(),
                Some(t!("set.ring_desc").to_string()),
                switch(ring, co).on_click(cx.listener(|t, _, _, cx| {
                    t.draft.history_ring = !t.draft.history_ring;
                    cx.notify();
                })),
                co,
            ))
            // Limit card (design `.limit-card`): dimmed + disabled when ring is off.
            .child(
                v_flex()
                    .gap(px(11.))
                    .p(px(14.))
                    .mt(px(14.))
                    .rounded(px(9.))
                    .border_1()
                    .border_color(co.border)
                    .bg(co.panel)
                    .when(!ring, |d| d.opacity(0.4))
                    .child(limit_line(
                        t!("set.limit").to_string(),
                        &self.mb,
                        "MB".to_string(),
                        ring,
                        co,
                    ))
                    .child(limit_line(
                        t!("set.limit").to_string(),
                        &self.min,
                        t!("set.minutes").to_string(),
                        ring,
                        co,
                    )),
            )
    }

    fn panel_profiling(&self, co: &Co, cx: &mut Context<Self>) -> impl IntoElement {
        let on = self.draft.profiling_enabled;
        let iv = self.draft.profiling_interval;
        v_flex()
            .child(section_title(t!("set.prof_title").to_string(), co))
            .child(lead(t!("set.prof_lead").to_string(), co))
            .child(set_row(
                t!("set.prof_enable").to_string(),
                None,
                switch(on, co).on_click(cx.listener(|t, _, _, cx| {
                    t.draft.profiling_enabled = !t.draft.profiling_enabled;
                    cx.notify();
                })),
                co,
            ))
            .child(set_row(
                t!("set.interval").to_string(),
                Some(t!("set.interval_desc").to_string()),
                div().when(!on, |d| d.opacity(0.4)).child(
                    seg(co)
                        .child(
                            seg_btn(
                                "iv-1",
                                t!("set.every_1s").to_string(),
                                iv == ProfilingInterval::OneSecond,
                                co,
                            )
                            .on_click(cx.listener(|t, _, _, cx| {
                                t.draft.profiling_interval = ProfilingInterval::OneSecond;
                                cx.notify();
                            })),
                        )
                        .child(
                            seg_btn(
                                "iv-100",
                                t!("set.every_100ms").to_string(),
                                iv == ProfilingInterval::HundredMs,
                                co,
                            )
                            .on_click(cx.listener(|t, _, _, cx| {
                                t.draft.profiling_interval = ProfilingInterval::HundredMs;
                                cx.notify();
                            })),
                        ),
                ),
                co,
            ))
    }

    fn panel_boot(&self, co: &Co, _cx: &mut Context<Self>) -> impl IntoElement {
        // Boot logging (the BOOT_START driver path) isn't supported yet, so the
        // toggle is shown disabled (dimmed, non-interactive) until it lands.
        v_flex()
            .child(section_title(t!("set.boot_title").to_string(), co))
            .child(lead(t!("set.boot_lead").to_string(), co))
            .child(set_row(
                t!("set.boot_enable").to_string(),
                Some(t!("set.boot_enable_desc").to_string()),
                switch_disabled(co),
                co,
            ))
            .child(note_warn(t!("set.boot_unsupported").to_string(), co))
    }

    fn panel_display(&self, co: &Co, cx: &mut Context<Self>) -> impl IntoElement {
        let (hx_off, hx_id) = (self.draft.hex_file_offset, self.draft.hex_thread_proc_id);
        let id_line = format!(
            "PID  {}   TID  {}",
            if hx_id { "0x400" } else { "1024" },
            if hx_id { "0x1546" } else { "5446" }
        );
        let off_line = format!(
            "Offset  {}   Length  {}",
            if hx_off { "0x8000" } else { "32768" },
            if hx_off { "0x1000" } else { "4096" }
        );
        v_flex()
            .child(section_title(t!("set.disp_title").to_string(), co).mb(px(8.)))
            .child(set_row(
                t!("set.hex_offset").to_string(),
                Some(t!("set.hex_offset_desc").to_string()),
                switch(hx_off, co).on_click(cx.listener(|t, _, _, cx| {
                    t.draft.hex_file_offset = !t.draft.hex_file_offset;
                    cx.notify();
                })),
                co,
            ))
            .child(set_row(
                t!("set.hex_id").to_string(),
                Some(t!("set.hex_id_desc").to_string()),
                switch(hx_id, co).on_click(cx.listener(|t, _, _, cx| {
                    t.draft.hex_thread_proc_id = !t.draft.hex_thread_proc_id;
                    cx.notify();
                })),
                co,
            ))
            .child(
                v_flex()
                    .mt(px(16.))
                    .rounded(px(9.))
                    .overflow_hidden()
                    .border_1()
                    .border_color(co.border)
                    .bg(cx.theme().background)
                    .child(preview_mono(id_line, co))
                    .child(
                        preview_mono(off_line, co)
                            .border_t_1()
                            .border_color(co.border),
                    ),
            )
    }
}

/// Resolved colors.
struct Co {
    fg: gpui_kit::Hsla,
    text2: gpui_kit::Hsla,
    muted: gpui_kit::Hsla,
    faint: gpui_kit::Hsla,
    border: gpui_kit::Hsla,
    border_soft: gpui_kit::Hsla,
    panel: gpui_kit::Hsla,
    accent: gpui_kit::Hsla,
    accent_soft: gpui_kit::Hsla,
    hover: gpui_kit::Hsla,
    warn: gpui_kit::Hsla,
}

impl Co {
    fn new(cx: &App) -> Self {
        let accent = cx.theme().primary;
        Self {
            fg: cx.theme().foreground,
            text2: cx.theme().foreground.opacity(0.72),
            muted: cx.theme().muted_foreground,
            faint: cx.theme().muted_foreground.opacity(0.55),
            border: cx.theme().border,
            border_soft: cx.theme().border.opacity(0.55),
            panel: cx.theme().secondary,
            accent,
            accent_soft: accent.opacity(0.16),
            hover: cx.theme().table_hover,
            warn: palette(cx).op_thread,
        }
    }
}

fn section_title(text: String, co: &Co) -> Div {
    div()
        .text_size(px(15.))
        .font_bold()
        .text_color(co.fg)
        .child(text)
}

fn lead(text: String, co: &Co) -> Div {
    div()
        .mt(px(6.))
        .mb(px(18.))
        .text_size(px(12.))
        .line_height(px(20.))
        .text_color(co.muted)
        .child(text)
}

fn fld_label(text: String, co: &Co) -> Div {
    div()
        .mt(px(14.))
        .mb(px(6.))
        .text_size(px(11.5))
        .text_color(co.muted)
        .child(text)
}

/// Forces a single-line `Input` to the design's 34px field height.
fn fld(input: Input) -> Input {
    input_h(input, px(34.))
}

/// A `.set-row`: title (+ desc) on the left, control on the right.
fn set_row(
    title: String,
    desc: Option<String>,
    control: impl IntoElement,
    co: &Co,
) -> impl IntoElement {
    h_flex()
        .items_center()
        .gap(px(16.))
        .py(px(13.))
        .border_b_1()
        .border_color(co.border_soft)
        .child(
            v_flex()
                .flex_1()
                .min_w(px(0.))
                .child(
                    div()
                        .text_size(px(12.5))
                        .font_medium()
                        .text_color(co.fg)
                        .child(title),
                )
                .when_some(desc, |t, d| {
                    t.child(
                        div()
                            .mt(px(3.))
                            .text_size(px(11.))
                            .text_color(co.muted)
                            .child(d),
                    )
                }),
        )
        .child(div().flex_shrink_0().child(control))
}

/// A `.set-row.full`: title/desc, then the control on its own line below.
fn set_row_full(
    title: String,
    desc: Option<String>,
    control: impl IntoElement,
    co: &Co,
) -> impl IntoElement {
    v_flex()
        .gap(px(11.))
        .py(px(13.))
        .border_b_1()
        .border_color(co.border_soft)
        .child(
            v_flex()
                .child(
                    div()
                        .text_size(px(12.5))
                        .font_medium()
                        .text_color(co.fg)
                        .child(title),
                )
                .when_some(desc, |t, d| {
                    t.child(
                        div()
                            .mt(px(3.))
                            .text_size(px(11.))
                            .text_color(co.muted)
                            .child(d),
                    )
                }),
        )
        .child(control)
}

/// A toggle (design `.switch`, 40×22). The caller adds `.id()`'s `on_click`.
fn switch(on: bool, co: &Co) -> Stateful<Div> {
    div()
        .id("switch")
        .w(px(40.))
        .h(px(22.))
        .rounded_full()
        .flex_shrink_0()
        .relative()
        .cursor_pointer()
        .bg(if on { co.accent } else { co.faint })
        .child(
            div()
                .absolute()
                .top(px(3.))
                .left(px(if on { 21. } else { 3. }))
                .size(px(16.))
                .rounded_full()
                .bg(white()),
        )
}

/// A non-interactive, dimmed off-switch for settings that aren't supported yet
/// (no `id`/`cursor`/`on_click`, so it can't be toggled).
fn switch_disabled(co: &Co) -> Div {
    div()
        .w(px(40.))
        .h(px(22.))
        .rounded_full()
        .flex_shrink_0()
        .relative()
        .bg(co.faint)
        .child(
            div()
                .absolute()
                .top(px(3.))
                .left(px(3.))
                .size(px(16.))
                .rounded_full()
                .bg(white().opacity(0.4)),
        )
}

/// A segmented-control container (design `.seg-ctl`): horizontal, boxed.
fn seg(co: &Co) -> Div {
    h_flex()
        .items_center()
        .gap(px(2.))
        .p(px(3.))
        .rounded(px(8.))
        .bg(co.panel)
        .border_1()
        .border_color(co.border)
}

/// A segmented button (design `.seg-btn`). Wrapped by [`seg`]; caller adds on_click.
fn seg_btn(id: &'static str, label: String, active: bool, co: &Co) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .px(px(13.))
        .py(px(5.))
        .rounded(px(6.))
        .cursor_pointer()
        .text_size(px(12.))
        .map(|d| {
            if active {
                d.bg(co.accent).text_color(white()).font_semibold()
            } else {
                d.text_color(co.text2).hover(|s| s.text_color(co.fg))
            }
        })
        .child(label)
}

/// A color swatch (design `.swatch`, 34×34). Caller adds on_click.
fn swatch(ix: usize, color: HighlightColor, selected: bool, co: &Co) -> Stateful<Div> {
    div()
        .id(("swatch", ix))
        .size(px(34.))
        .rounded(px(9.))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .border_2()
        .border_color(if selected { co.fg } else { transparent_black() })
        .bg(color.hsla())
        .when(selected, |d| {
            d.child(Icon::new(PmIcon::Check).size(px(14.)).text_color(black()))
        })
}

/// A `.limit-line`: label + numeric field + unit. Field disabled when `enabled` off.
fn limit_line(
    label: String,
    input: &Entity<InputState>,
    unit: String,
    enabled: bool,
    co: &Co,
) -> impl IntoElement {
    h_flex()
        .items_center()
        .gap(px(11.))
        .text_size(px(12.5))
        .child(div().w(px(42.)).text_color(co.text2).child(label))
        .child(fld(Input::new(input).disabled(!enabled)).w(px(120.)))
        .child(div().text_color(co.muted).child(unit))
}

/// A warning note (design `.set-note.warn`).
fn note_warn(text: String, co: &Co) -> impl IntoElement {
    h_flex()
        .items_center()
        .gap(px(9.))
        .mt(px(16.))
        .px(px(13.))
        .py(px(10.))
        .rounded(px(8.))
        .text_size(px(11.5))
        .text_color(co.warn)
        .bg(co.warn.opacity(0.14))
        .border_1()
        .border_color(co.warn.opacity(0.30))
        .child(Icon::new(PmIcon::Info).size(px(14.)))
        .child(text)
}

/// A `.set-preview.mono` row.
fn preview_mono(text: String, co: &Co) -> Div {
    div()
        .px(px(13.))
        .py(px(9.))
        .text_size(px(11.5))
        .font_family("Consolas")
        .text_color(co.fg)
        .child(text)
}

/// Parses a positive integer, clamped to >= 1, falling back to `fallback`.
fn parse_min1(text: &str, fallback: usize) -> usize {
    text.trim().parse::<usize>().unwrap_or(fallback).max(1)
}
