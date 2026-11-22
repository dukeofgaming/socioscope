use std::process::Command;
use std::time::Instant;

use gpui::{
    App, Context, Entity, FocusHandle, Focusable, FontWeight, KeyBinding, PathPromptOptions,
    ScrollHandle, Stateful, Subscription, Task, Window, div, prelude::*, px, relative,
};
use gpui_ce_elements::editable_text::{
    EditableTextState, StringStorage, TextChanged, actions::default_bindings, text_input,
};
use gpui_ce_elements::editable_text::actions::DEFAULT_INPUT_CONTEXT;

use crate::library::{self, Recording};
use crate::srt::{self, Cue};
use crate::theme::{self, Theme, speaker_color, with_alpha};

gpui::actions!(
    socioscope,
    [
        TogglePlay,
        PreviousCue,
        NextCue,
        PreviousRecording,
        NextRecording
    ]
);

const ROW: f32 = 36.0;

pub struct Studio {
    recordings: Vec<Recording>,
    selected: Option<String>,
    playhead: f64,
    playing: bool,
    speed: f64,
    follow: bool,
    zoom_ms: Option<f64>,
    origin: f64,
    last_tick: Instant,
    speaker: Option<String>,
    picked: Option<usize>,
    tracked: Option<usize>,
    file_tab: bool,
    query: Entity<EditableTextState>,
    focus: FocusHandle,
    sheet: ScrollHandle,
    toast: Option<String>,
    log: Vec<String>,
    busy: bool,
    job: Option<Task<()>>,
    peaks_job: Option<Task<()>>,
    peaks_requested: Vec<String>,
    _subscription: Subscription,
}

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys(default_bindings().as_keybindings(Some(DEFAULT_INPUT_CONTEXT)));
    cx.bind_keys([
        KeyBinding::new("space", TogglePlay, Some("Socioscope")),
        KeyBinding::new("left", PreviousCue, Some("Socioscope")),
        KeyBinding::new("right", NextCue, Some("Socioscope")),
        KeyBinding::new("up", PreviousRecording, Some("Socioscope")),
        KeyBinding::new("down", NextRecording, Some("Socioscope")),
    ]);
}

impl Studio {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let query = cx.new(|cx| EditableTextState::new(StringStorage::default(), cx));
        let subscription = cx.subscribe(&query, |_, _, _: &TextChanged, cx| cx.notify());
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let recordings = library::load_library();
        let selected = recordings.first().map(|recording| recording.id.clone());
        Self {
            recordings,
            selected,
            playhead: 0.0,
            playing: false,
            speed: 1.0,
            follow: true,
            zoom_ms: Some(90_000.0),
            origin: 0.0,
            last_tick: Instant::now(),
            speaker: None,
            picked: None,
            tracked: None,
            file_tab: false,
            query,
            focus,
            sheet: ScrollHandle::new(),
            toast: None,
            log: Vec::new(),
            busy: false,
            job: None,
            peaks_job: None,
            peaks_requested: Vec::new(),
            _subscription: subscription,
        }
    }

    fn kick_peaks(&mut self, cx: &mut Context<Self>) {
        let Some((id, source)) = self.current().and_then(|recording| {
            if recording.peaks.is_empty() {
                recording.source.clone().map(|source| (recording.id.clone(), source))
            } else {
                None
            }
        }) else {
            return;
        };
        if self.peaks_requested.iter().any(|item| item == &id) {
            return;
        }
        self.peaks_requested.push(id.clone());
        self.peaks_job = Some(cx.spawn(async move |this, async_cx| {
            let peaks = async_cx
                .background_spawn(async move { library::waveform(&source, 960) })
                .await;
            this.update(async_cx, |this, cx| {
                if let Some(recording) = this.recordings.iter_mut().find(|item| item.id == id) {
                    recording.peaks = peaks;
                }
                cx.notify();
            })
            .ok();
        }));
    }

    fn tick(&mut self, window: &mut Window) {
        if !self.playing {
            return;
        }
        let now = Instant::now();
        let dt = now.saturating_duration_since(self.last_tick).as_secs_f64().min(0.1);
        self.last_tick = now;
        let duration = self.duration();
        self.playhead = (self.playhead + dt * 1000.0 * self.speed).min(duration);
        if self.playhead >= duration {
            self.playing = false;
            return;
        }
        if self.follow {
            self.reveal(self.playhead);
        }
        let active = self.cue_at(self.playhead);
        if active != self.tracked {
            self.tracked = active;
            if let Some(index) = active {
                self.scroll_to(index);
            }
        }
        window.request_animation_frame();
    }

    fn duration(&self) -> f64 {
        self.current().map(|recording| recording.duration_ms as f64).unwrap_or(0.0)
    }

    fn span(&self) -> f64 {
        let duration = self.duration().max(1.0);
        self.zoom_ms.map(|ms| ms.min(duration)).unwrap_or(duration).max(1.0)
    }

    fn current(&self) -> Option<&Recording> {
        let id = self.selected.as_ref()?;
        self.recordings.iter().find(|recording| &recording.id == id)
    }

    fn visible(&self, query: &str) -> Vec<usize> {
        let needle = query.trim().to_lowercase();
        self.recordings
            .iter()
            .enumerate()
            .filter(|(_, recording)| {
                needle.is_empty()
                    || recording.title.to_lowercase().contains(&needle)
                    || recording.detail.to_lowercase().contains(&needle)
            })
            .map(|(index, _)| index)
            .collect()
    }

    fn reveal(&mut self, ms: f64) {
        let span = self.span();
        let duration = self.duration();
        if ms < self.origin || ms > self.origin + span {
            let max_origin = (duration - span).max(0.0);
            self.origin = (ms - span * 0.28).clamp(0.0, max_origin);
        }
    }

    fn scroll_to(&self, index: usize) {
        self.sheet.scroll_to_item(index);
    }

    fn search_focused(&self, window: &Window, cx: &App) -> bool {
        self.query.read(cx).focus_handle(cx).is_focused(window)
    }

    fn toggle_play(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search_focused(window, cx) || self.duration() <= 0.0 {
            return;
        }
        self.playing = !self.playing;
        self.last_tick = Instant::now();
        if self.playing && self.playhead >= self.duration() {
            self.playhead = 0.0;
            self.origin = 0.0;
        }
        cx.notify();
    }

    fn choose(&mut self, id: &str, cx: &mut Context<Self>) {
        self.selected = Some(id.to_string());
        self.playing = false;
        self.picked = None;
        self.tracked = None;
        self.speaker = None;
        self.origin = 0.0;
        self.playhead = self
            .current()
            .and_then(|recording| recording.cues.first().map(|cue| cue.start_ms as f64))
            .unwrap_or(0.0);
        self.sheet.scroll_to_top_of_item(0);
        cx.notify();
    }

    fn step_recording(&mut self, delta: isize, cx: &mut Context<Self>) {
        let query = self.query.read(cx).as_str().to_string();
        let ids: Vec<String> = self
            .visible(&query)
            .into_iter()
            .map(|index| self.recordings[index].id.clone())
            .collect();
        if ids.is_empty() {
            return;
        }
        let pos = self
            .selected
            .as_ref()
            .and_then(|id| ids.iter().position(|item| item == id))
            .unwrap_or(0);
        let next = (pos as isize + delta).clamp(0, ids.len() as isize - 1) as usize;
        self.choose(&ids[next], cx);
    }

    fn seek(&mut self, ms: f64, cx: &mut Context<Self>) {
        self.playhead = ms.clamp(0.0, self.duration());
        self.reveal(self.playhead);
        self.tracked = None;
        cx.notify();
    }

    fn seek_cue(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(start) = self.current().and_then(|recording| recording.cues.get(index).map(|cue| cue.start_ms as f64)) else {
            return;
        };
        self.picked = Some(index);
        self.seek(start + 8.0, cx);
        self.scroll_to(index);
    }

    fn step_cue(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some(len) = self.current().map(|recording| recording.cues.len()) else {
            return;
        };
        if len == 0 {
            return;
        }
        let current = self.picked.or_else(|| self.cue_at(self.playhead)).unwrap_or(0);
        let next = (current as isize + delta).clamp(0, len as isize - 1) as usize;
        self.seek_cue(next, cx);
    }

    fn cue_at(&self, time: f64) -> Option<usize> {
        let cues = &self.current()?.cues;
        let ms = time.max(0.0) as u64;
        cues.iter()
            .enumerate()
            .rev()
            .find(|(_, cue)| cue.start_ms <= ms && ms < cue.end_ms)
            .map(|(index, _)| index)
    }

    fn upcoming(&self) -> Option<usize> {
        let cues = &self.current()?.cues;
        let ms = self.playhead.max(0.0) as u64;
        cues.iter().position(|cue| cue.start_ms > ms)
    }

    fn inspected(&self) -> Option<usize> {
        if self.playing {
            self.cue_at(self.playhead).or(self.picked)
        } else {
            self.picked.or_else(|| self.cue_at(self.playhead)).or_else(|| self.upcoming())
        }
    }

    fn reload(&mut self) {
        let selected = self.selected.clone();
        self.recordings = library::load_library();
        if let Some(id) = selected {
            if self.recordings.iter().any(|recording| recording.id == id) {
                self.selected = Some(id);
                return;
            }
        }
        self.selected = self.recordings.first().map(|recording| recording.id.clone());
    }

    fn import_srt(&mut self, cx: &mut Context<Self>) {
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Import an SRT".into()),
        });
        self.job = Some(cx.spawn(async move |this, async_cx| {
            let chosen = prompt.await;
            this.update(async_cx, |this, cx| {
                match chosen {
                    Ok(Ok(Some(paths))) => {
                        if let Some(path) = paths.first() {
                            match library::import_srt(path) {
                                Ok(id) => {
                                    this.reload();
                                    this.choose(&id, cx);
                                    this.toast = Some("Captions imported.".into());
                                }
                                Err(err) => this.toast = Some(err),
                            }
                        }
                    }
                    Ok(Err(err)) => this.toast = Some(format!("Could not open a file dialog. {err}")),
                    _ => {}
                }
                cx.notify();
            })
            .ok();
        }));
    }

    fn transcribe(&mut self, cx: &mut Context<Self>) {
        let Some(source) = self.current().and_then(|recording| recording.source.clone()) else {
            self.toast = Some("This recording has no audio file.".into());
            cx.notify();
            return;
        };
        if self.busy {
            return;
        }
        self.busy = true;
        self.toast = Some("Transcribing on this machine…".into());
        let root = library::repo_root();
        cx.notify();
        self.job = Some(cx.spawn(async move |this, async_cx| {
            let result = async_cx
                .background_spawn(async move {
                    Command::new("python3")
                        .args(["-m", "socioscope", "transcribe", &source.to_string_lossy()])
                        .current_dir(root)
                        .output()
                })
                .await;
            this.update(async_cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(output) if output.status.success() => {
                        this.toast = Some("Transcription finished.".into());
                        this.log.clear();
                        this.reload();
                    }
                    Ok(output) => {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        let text = if stderr.trim().is_empty() { stdout } else { stderr };
                        let lines: Vec<String> = text.lines().map(str::trim).filter(|line| !line.is_empty()).map(str::to_string).collect();
                        this.toast = Some(
                            lines
                                .last()
                                .cloned()
                                .unwrap_or_else(|| "Transcription failed.".into()),
                        );
                        self_log(this, lines);
                    }
                    Err(err) => this.toast = Some(err.to_string()),
                }
                cx.notify();
            })
            .ok();
        }));
    }
}

fn self_log(studio: &mut Studio, lines: Vec<String>) {
    let start = lines.len().saturating_sub(6);
    studio.log = lines[start..].to_vec();
}

impl Render for Studio {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.tick(window);
        self.kick_peaks(cx);
        let theme = theme::theme();
        let query = self.query.read(cx).as_str().to_string();
        let app = cx.weak_entity();

        div()
            .id("studio")
            .size_full()
            .flex()
            .bg(theme.bg)
            .text_color(theme.text)
            .font_family("Inter")
            .text_size(px(13.))
            .overflow_hidden()
            .track_focus(&self.focus)
            .key_context("Socioscope")
            .on_action(cx.listener(|this, _: &TogglePlay, window, cx| this.toggle_play(window, cx)))
            .on_action(cx.listener(|this, _: &PreviousCue, window, cx| {
                if !this.search_focused(window, cx) {
                    this.step_cue(-1, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &NextCue, window, cx| {
                if !this.search_focused(window, cx) {
                    this.step_cue(1, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &PreviousRecording, window, cx| {
                if !this.search_focused(window, cx) {
                    this.step_recording(-1, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &NextRecording, window, cx| {
                if !this.search_focused(window, cx) {
                    this.step_recording(1, cx);
                }
            }))
            .child(self.sidebar(&query, app.clone(), theme, cx))
            .child(self.stage(&query, app.clone(), theme, cx))
            .child(self.inspector(app, theme, cx))
    }
}

impl Studio {
    fn sidebar(
        &self,
        query: &str,
        app: gpui::WeakEntity<Self>,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let visible = self.visible(query);
        let mut list = div()
            .id("library")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .pt(px(8.));
        for index in visible {
            let recording = &self.recordings[index];
            let selected = self.selected.as_deref() == Some(recording.id.as_str());
            let (fast, short, overlap) = srt::review_counts(&recording.cues);
            let issues = fast + short + overlap;
            let id = recording.id.clone();
            let app = app.clone();
            let mut row = div()
                .id(format!("lib-{id}"))
                .mx(px(8.))
                .mb(px(2.))
                .px(px(10.))
                .py(px(8.))
                .rounded(px(8.))
                .flex()
                .items_center()
                .gap(px(10.))
                .cursor_pointer()
                .on_click(move |_event, _window, cx| {
                    app.update(cx, |this, cx| this.choose(&id, cx)).ok();
                });
            if selected {
                row = row.bg(theme.raised);
            } else {
                row = row.hover(|style| style.bg(theme.raised));
            }
            let dot = if recording.cues.is_empty() {
                theme.faint
            } else if issues > 0 {
                theme.warn
            } else {
                theme.bone
            };
            list = list.child(
                row.child(div().w(px(6.)).h(px(6.)).rounded_full().bg(dot).flex_none())
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(
                                div()
                                    .truncate()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(recording.title.clone()),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(11.))
                                    .text_color(theme.faint)
                                    .child(library_meta(recording, issues)),
                            ),
                    )
                    .child(
                        div()
                            .font_family("JetBrains Mono")
                            .text_size(px(11.))
                            .text_color(theme.faint)
                            .flex_none()
                            .child(srt::clock(recording.duration_ms)),
                    ),
            );
        }

        let import = app.clone();
        div()
            .w(px(260.))
            .h_full()
            .flex()
            .flex_col()
            .flex_none()
            .border_r_1()
            .border_color(theme.line)
            .child(self.brand(theme))
            .child(self.search(theme, cx))
            .child(
                div()
                    .px(px(18.))
                    .pt(px(16.))
                    .pb(px(4.))
                    .text_size(px(10.))
                    .text_color(theme.faint)
                    .font_weight(FontWeight::SEMIBOLD)
                    .letter_spacing(px(1.1))
                    .child("LIBRARY"),
            )
            .child(list)
            .child(sidebar_footer(import, theme, self.toast.clone()))
    }

    fn brand(&self, theme: Theme) -> gpui::Div {
        div()
            .px(px(16.))
            .pt(px(16.))
            .pb(px(12.))
            .flex()
            .items_center()
            .gap(px(10.))
            .child(mark(theme))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(14.))
                            .child("Socioscope"),
                    )
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(theme.faint)
                            .letter_spacing(px(1.2))
                            .child("CAPTION STUDIO"),
                    ),
            )
    }

    fn search(&self, theme: Theme, _cx: &mut Context<Self>) -> gpui::Div {
        div()
            .px(px(12.))
            .child(
                text_input("library-search")
                    .state(self.query.downgrade())
                    .placeholder("Search recordings")
                    .placeholder_color(theme.faint)
                    .caret_color(gpui::rgb_to_hsla(theme.bone))
                    .caret_blink_interval_500ms()
                    .w_full()
                    .min_h_auto()
                    .max_h_auto()
                    .whitespace_nowrap()
                    .px(px(10.))
                    .py(px(6.))
                    .rounded(px(7.))
                    .text_size(px(12.))
                    .border_1()
                    .border_color(theme.line)
                    .bg(theme.stage),
            )
    }

    fn stage(
        &self,
        _query: &str,
        app: gpui::WeakEntity<Self>,
        theme: Theme,
        _cx: &mut Context<Self>,
    ) -> gpui::Div {
        let Some(recording) = self.current() else {
            return empty_stage(theme);
        };
        let duration = recording.duration_ms as f64;
        let span = self.span();
        let origin = self.origin.min((duration - span).max(0.0));
        let inspected = self.inspected();
        let active = self.cue_at(self.playhead);

        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(theme.stage)
            .child(stage_header(recording, theme))
            .child(monitor(recording, inspected, self.playhead, theme))
            .child(self.transport(app.clone(), theme))
            .child(timeline(
                recording,
                origin,
                span,
                self.playhead,
                self.speaker.as_deref(),
                self.picked,
                app.clone(),
                theme,
            ))
            .child(cue_sheet(
                recording,
                active,
                self.picked,
                self.speaker.as_deref(),
                &self.sheet,
                app,
                theme,
            ))
    }

    fn transport(&self, app: gpui::WeakEntity<Self>, theme: Theme) -> gpui::Div {
        let duration = self.duration();
        let play = app.clone();
        let previous = app.clone();
        let next = app.clone();
        let follow = app.clone();
        let follow_on = self.follow;
        let mut row = div()
            .h(px(48.))
            .px(px(20.))
            .flex()
            .items_center()
            .gap(px(8.))
            .border_b_1()
            .border_color(theme.line)
            .child(transport_button("cue-prev", "Prev", previous, theme, |this, cx| {
                this.step_cue(-1, cx);
            }))
            .child(transport_button(
                "play",
                if self.playing { "Pause" } else { "Play" },
                play,
                theme,
                |this, cx| {
                    this.playing = !this.playing;
                    this.last_tick = Instant::now();
                    if this.playing && this.playhead >= this.duration() {
                        this.playhead = 0.0;
                        this.origin = 0.0;
                    }
                    cx.notify();
                },
            ))
            .child(transport_button("cue-next", "Next", next, theme, |this, cx| {
                this.step_cue(1, cx);
            }))
            .child(
                div()
                    .ml(px(8.))
                    .font_family("JetBrains Mono")
                    .text_size(px(13.))
                    .child(srt::transport_clock(self.playhead)),
            )
            .child(
                div()
                    .font_family("JetBrains Mono")
                    .text_size(px(12.))
                    .text_color(theme.faint)
                    .child(format!("/  {}", srt::transport_clock(duration))),
            )
            .child(div().flex_1());

        for (id, label, value) in [("s08", "0.8×", 0.8), ("s10", "1×", 1.0), ("s12", "1.25×", 1.25), ("s15", "1.5×", 1.5)] {
            let app = app.clone();
            let on = (self.speed - value).abs() < 0.01;
            row = row.child(chip(id, label, on, theme).on_click(move |_event, _window, cx| {
                app.update(cx, |this, cx| {
                    this.speed = value;
                    cx.notify();
                })
                .ok();
            }));
        }
        row = row.child(chip("follow", "Follow", follow_on, theme).on_click(move |_event, _window, cx| {
            follow
                .update(cx, |this, cx| {
                    this.follow = !this.follow;
                    cx.notify();
                })
                .ok();
        }));
        for (id, label, zoom) in [
            ("z30", "30s", Some(30_000.0)),
            ("z90", "90s", Some(90_000.0)),
            ("z3m", "3m", Some(180_000.0)),
            ("zall", "All", None),
        ] {
            let app = app.clone();
            let on = self.zoom_ms == zoom;
            row = row.child(chip(id, label, on, theme).on_click(move |_event, _window, cx| {
                app.update(cx, |this, cx| {
                    this.zoom_ms = zoom;
                    this.reveal(this.playhead);
                    cx.notify();
                })
                .ok();
            }));
        }
        row
    }

    fn inspector(&self, app: gpui::WeakEntity<Self>, theme: Theme, _cx: &mut Context<Self>) -> gpui::Div {
        let recording = self.current().cloned();
        let mut panel = div()
            .w(px(320.))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .bg(theme.bg)
            .border_l_1()
            .border_color(theme.line);
        let cue_tab = app.clone();
        let file_tab = app.clone();
        panel = panel.child(
            div()
                .px(px(16.))
                .pt(px(14.))
                .pb(px(10.))
                .flex()
                .gap(px(6.))
                .child(chip("tab-cue", "Cue", !self.file_tab, theme).on_click(move |_e, _w, cx| {
                    cue_tab.update(cx, |this, cx| {
                        this.file_tab = false;
                        cx.notify();
                    }).ok();
                }))
                .child(chip("tab-file", "File", self.file_tab, theme).on_click(move |_e, _w, cx| {
                    file_tab.update(cx, |this, cx| {
                        this.file_tab = true;
                        cx.notify();
                    }).ok();
                })),
        );
        if self.file_tab {
            panel.child(file_panel(recording.as_ref(), &self.log, self.busy, app, theme))
        } else {
            panel.child(cue_panel(recording.as_ref(), self.inspected(), theme))
        }
    }
}

fn empty_stage(theme: Theme) -> gpui::Div {
    div()
        .flex_1()
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(theme.stage)
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(8.))
                .child(div().font_weight(FontWeight::SEMIBOLD).text_size(px(18.)).child("No recordings"))
                .child(
                    div()
                        .text_color(theme.muted)
                        .text_size(px(13.))
                        .child("Import an SRT to open the caption visualizer."),
                ),
        )
}

fn stage_header(recording: &Recording, theme: Theme) -> gpui::Div {
    let (fast, short, overlap) = srt::review_counts(&recording.cues);
    let mut review = Vec::new();
    if fast > 0 {
        review.push(format!("{fast} over reading speed"));
    }
    if short > 0 {
        review.push(format!("{short} under a second"));
    }
    if overlap > 0 {
        review.push(format!("{overlap} overlapping"));
    }
    let file = recording
        .srt_path
        .as_ref()
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("no captions");
    let covered = recording.cues.last().map(|cue| srt::clock(cue.end_ms));
    let mut meta = format!("{} cues · {}", recording.cues.len(), srt::clock(recording.duration_ms));
    if let Some(end) = covered {
        if recording.cues.last().map(|cue| cue.end_ms + 1500 < recording.duration_ms).unwrap_or(false) {
            meta = format!("{meta} · captions through {end}");
        }
    }
    meta = format!("{meta} · {file}");
    if recording.sample {
        meta = format!("{meta} · sample track");
    }
    if recording.from_pipeline {
        meta = format!("{meta} · pipeline");
    }

    div()
        .px(px(24.))
        .pt(px(16.))
        .pb(px(12.))
        .flex()
        .flex_col()
        .gap(px(4.))
        .child(
            div()
                .text_size(px(20.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(recording.title.clone()),
        )
        .child(div().text_size(px(12.)).text_color(theme.muted).child(recording.detail.clone()))
        .child(
            div()
                .flex()
                .gap(px(12.))
                .text_size(px(12.))
                .child(div().text_color(theme.faint).child(meta))
                .when(!review.is_empty(), |row| {
                    row.child(div().text_color(theme.warn).child(review.join("  ·  ")))
                }),
        )
}

fn monitor(recording: &Recording, inspected: Option<usize>, playhead: f64, theme: Theme) -> gpui::Div {
    let cue = inspected.and_then(|index| recording.cues.get(index));
    let active = cue.filter(|cue| cue.start_ms as f64 <= playhead && playhead < cue.end_ms as f64);
    let upcoming = cue.filter(|_| active.is_none());
    let speaker = cue.and_then(|cue| cue.speaker.clone()).unwrap_or_default();
    let color = speaker_color(&speaker);
    let lines: Vec<String> = cue
        .map(|cue| cue.text.lines().map(str::to_string).collect())
        .unwrap_or_default();

    let mut screen = div()
        .size_full()
        .relative()
        .bg(theme.monitor)
        .child(corner(true, true))
        .child(corner(false, true))
        .child(corner(true, false))
        .child(corner(false, false))
        .child(
            div()
                .absolute()
                .top(px(14.))
                .left(px(16.))
                .font_family("JetBrains Mono")
                .text_size(px(11.))
                .text_color(rgb_quiet())
                .child(srt::transport_clock(playhead)),
        )
        .child(
            div()
                .absolute()
                .top(px(14.))
                .right(px(16.))
                .font_family("JetBrains Mono")
                .text_size(px(11.))
                .text_color(rgb_quiet())
                .child(
                    cue.map(|cue| format!("CUE {:02}", cue.index))
                        .unwrap_or_else(|| "CUE —".into()),
                ),
        );

    let mut stack = div()
        .absolute()
        .bottom(px(28.))
        .left(px(48.))
        .right(px(48.))
        .flex()
        .flex_col()
        .items_center()
        .gap(px(6.));
    if let Some(cue) = upcoming {
        let wait = (cue.start_ms as f64 - playhead).max(0.0) / 1000.0;
        stack = stack.child(
            div()
                .font_family("JetBrains Mono")
                .text_size(px(11.))
                .text_color(rgb_quiet())
                .child(format!("UP NEXT  {:.1}s", wait)),
        );
    }
    if !speaker.is_empty() {
        stack = stack.child(
            div()
                .text_size(px(11.))
                .font_weight(FontWeight::SEMIBOLD)
                .letter_spacing(px(1.1))
                .text_color(color)
                .child(speaker.to_uppercase()),
        );
    }
    if lines.is_empty() {
        stack = stack.child(
            div()
                .text_size(px(16.))
                .text_color(theme.muted)
                .child(if recording.cues.is_empty() {
                    "No captions on this recording"
                } else {
                    "End of captions"
                }),
        );
    }
    for line in lines {
        stack = stack.child(
            div()
                .px(px(10.))
                .py(px(2.))
                .rounded(px(3.))
                .bg(with_alpha(theme.monitor, 0.0))
                .child(
                    div()
                        .px(px(8.))
                        .py(px(1.))
                        .bg(gpui::rgba(0x000000B0))
                        .text_size(px(22.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.bone)
                        .child(line),
                ),
        );
    }
    if upcoming.is_some() {
        stack = stack.opacity(0.45);
    }
    screen = screen.child(stack);

    div()
        .px(px(24.))
        .pb(px(4.))
        .child(
            div()
                .h(px(228.))
                .w_full()
                .rounded(px(12.))
                .bg(rgb_bezel())
                .border_1()
                .border_color(theme.line)
                .p(px(8.))
                .child(screen),
        )
}

fn timeline(
    recording: &Recording,
    origin: f64,
    span: f64,
    playhead: f64,
    speaker_filter: Option<&str>,
    picked: Option<usize>,
    app: gpui::WeakEntity<Studio>,
    theme: Theme,
) -> gpui::Div {
    let duration = recording.duration_ms.max(1) as f64;
    let lane_of = srt::lanes(&recording.cues);
    let lane_count = lane_of.iter().copied().max().unwrap_or(0).min(3) + 1;
    let end = origin + span;
    let bars = waveform_bars(&recording.peaks, origin, span, duration, 120);

    let mut wave = div().h(px(36.)).w_full().flex().items_end().gap(px(1.));
    for (index, peak) in bars.iter().enumerate() {
        let center = origin + span * ((index as f64 + 0.5) / bars.len().max(1) as f64);
        let hot = (center - playhead).abs() < span / bars.len().max(1) as f64;
        wave = wave.child(
            div()
                .flex_1()
                .h(relative(peak.max(0.04)))
                .rounded(px(1.))
                .bg(if hot { theme.bone } else { theme.line }),
        );
    }

    let mut lanes = div()
        .relative()
        .w_full()
        .h(px(lane_count as f32 * 26.0))
        .mt(px(6.));
    for (index, cue) in recording.cues.iter().enumerate() {
        if cue.end_ms as f64 <= origin || cue.start_ms as f64 >= end {
            continue;
        }
        let left = ((cue.start_ms as f64).max(origin) - origin) / span;
        let right = ((cue.end_ms as f64).min(end) - origin) / span;
        let width = (right - left).max(0.004);
        let name = cue.speaker.clone().unwrap_or_default();
        let dim = speaker_filter.is_some() && speaker_filter != Some(name.as_str());
        let note = srt::assess(&recording.cues, index);
        let color = if note.fast || note.short { theme.warn } else { speaker_color(&name) };
        let lane = lane_of[index].min(3);
        let app = app.clone();
        let mut block = div()
            .id(format!("block-{index}"))
            .absolute()
            .left(relative(left as f32))
            .w(relative(width as f32))
            .top(px(lane as f32 * 26.0 + 3.0))
            .h(px(18.))
            .rounded(px(4.))
            .bg(color)
            .cursor_pointer()
            .opacity(if dim { 0.2 } else { 0.92 })
            .on_click(move |_event, _window, cx| {
                app.update(cx, |this, cx| this.seek_cue(index, cx)).ok();
            });
        if picked == Some(index) {
            block = block.border_1().border_color(theme.bone);
        }
        lanes = lanes.child(block);
    }

    let mut ruler = div().relative().h(px(16.)).w_full();
    let step = ruler_step(span);
    let mut tick = (origin / step).ceil() * step;
    while tick < end {
        let x = ((tick - origin) / span) as f32;
        ruler = ruler.child(
            div()
                .absolute()
                .left(relative(x))
                .top(px(0.))
                .font_family("JetBrains Mono")
                .text_size(px(10.))
                .text_color(theme.faint)
                .child(srt::clock(tick as u64)),
        );
        tick += step;
    }
    ruler = ruler.child(seek_strip("ruler", origin, span, 24, app.clone()));

    let head = ((playhead - origin) / span).clamp(0.0, 1.0) as f32;
    let stack = div()
        .relative()
        .w_full()
        .child(ruler)
        .child(wave)
        .child(lanes)
        .child(
            div()
                .absolute()
                .top(px(0.))
                .bottom(px(0.))
                .left(relative(head))
                .w(px(1.))
                .bg(theme.bone),
        );

    let map = minimap(recording, origin, span, playhead, theme, app.clone());

    div()
        .px(px(24.))
        .pt(px(10.))
        .pb(px(8.))
        .flex()
        .flex_col()
        .border_b_1()
        .border_color(theme.line)
        .child(legend(recording, speaker_filter, app, theme))
        .child(stack)
        .child(map)
}

fn legend(
    recording: &Recording,
    filter: Option<&str>,
    app: gpui::WeakEntity<Studio>,
    theme: Theme,
) -> gpui::Div {
    let mut names: Vec<String> = Vec::new();
    for cue in &recording.cues {
        if let Some(name) = cue.speaker.clone() {
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }
    let mut row = div().flex().items_center().gap(px(6.)).pb(px(8.)).flex_wrap();
    if names.is_empty() {
        return row.child(div().text_size(px(11.)).text_color(theme.faint).child("No speaker labels in this SRT"));
    }
    for name in names {
        let count = recording
            .cues
            .iter()
            .filter(|cue| cue.speaker.as_deref() == Some(name.as_str()))
            .count();
        let on = filter == Some(name.as_str());
        let app = app.clone();
        let key = name.clone();
        row = row.child(
            div()
                .id(format!("speaker-{name}"))
                .flex()
                .items_center()
                .gap(px(6.))
                .px(px(8.))
                .py(px(3.))
                .rounded_full()
                .cursor_pointer()
                .border_1()
                .border_color(if on { theme.bone } else { theme.line })
                .hover(|style| style.bg(theme.raised))
                .on_click(move |_event, _window, cx| {
                    app.update(cx, |this, cx| {
                        if this.speaker.as_deref() == Some(key.as_str()) {
                            this.speaker = None;
                        } else {
                            this.speaker = Some(key.clone());
                        }
                        cx.notify();
                    })
                    .ok();
                })
                .child(div().w(px(6.)).h(px(6.)).rounded_full().bg(speaker_color(&name)))
                .child(div().text_size(px(11.)).child(name))
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(theme.faint)
                        .font_family("JetBrains Mono")
                        .child(count.to_string()),
                ),
        );
    }
    row
}

fn seek_strip(
    prefix: &str,
    origin: f64,
    span: f64,
    zones: usize,
    app: gpui::WeakEntity<Studio>,
) -> gpui::Div {
    let mut strip = div().absolute().top(px(0.)).left(px(0.)).right(px(0.)).bottom(px(0.)).flex();
    for index in 0..zones {
        let app = app.clone();
        let at = origin + span * ((index as f64 + 0.5) / zones as f64);
        strip = strip.child(
            div()
                .id(format!("{prefix}-{index}"))
                .flex_1()
                .h_full()
                .cursor_pointer()
                .on_click(move |_event, _window, cx| {
                    app.update(cx, |this, cx| this.seek(at, cx)).ok();
                }),
        );
    }
    strip
}

fn minimap(
    recording: &Recording,
    origin: f64,
    span: f64,
    playhead: f64,
    theme: Theme,
    app: gpui::WeakEntity<Studio>,
) -> gpui::Div {
    let duration = recording.duration_ms.max(1) as f64;
    let mut map = div()
        .relative()
        .h(px(18.))
        .mt(px(8.))
        .w_full()
        .rounded(px(4.))
        .bg(theme.bg)
        .overflow_hidden();
    for cue in &recording.cues {
        let left = cue.start_ms as f64 / duration;
        let width = (cue.duration_ms() as f64 / duration).max(0.002);
        let name = cue.speaker.clone().unwrap_or_default();
        map = map.child(
            div()
                .absolute()
                .left(relative(left as f32))
                .w(relative(width as f32))
                .top(px(5.))
                .h(px(8.))
                .rounded(px(2.))
                .bg(speaker_color(&name)),
        );
    }
    let view_left = (origin / duration) as f32;
    let view_width = (span / duration).clamp(0.01, 1.0) as f32;
    map = map
        .child(
            div()
                .absolute()
                .top(px(0.))
                .bottom(px(0.))
                .left(relative(view_left))
                .w(relative(view_width))
                .border_1()
                .border_color(theme.bone)
                .bg(with_alpha(theme.bone, 0.08)),
        )
        .child(
            div()
                .absolute()
                .top(px(0.))
                .bottom(px(0.))
                .left(relative((playhead / duration).clamp(0.0, 1.0) as f32))
                .w(px(1.))
                .bg(theme.bone),
        )
        .child(seek_strip("map", 0.0, duration, 32, app));
    map
}

fn cue_sheet(
    recording: &Recording,
    active: Option<usize>,
    picked: Option<usize>,
    filter: Option<&str>,
    sheet: &ScrollHandle,
    app: gpui::WeakEntity<Studio>,
    theme: Theme,
) -> gpui::Div {
    let highlight = if recording.cues.is_empty() {
        None
    } else {
        active.or(picked)
    };
    let mut rows = div()
        .id("cue-sheet")
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .track_scroll(sheet);
    if recording.cues.is_empty() {
        rows = rows.child(
            div()
                .px(px(24.))
                .py(px(18.))
                .text_color(theme.faint)
                .child("The cue sheet fills in when an SRT is loaded."),
        );
    }
    for (index, cue) in recording.cues.iter().enumerate() {
        let name = cue.speaker.clone().unwrap_or_default();
        let dim = filter.is_some() && filter != Some(name.as_str());
        let note = srt::assess(&recording.cues, index);
        let on = highlight == Some(index);
        let app = app.clone();
        let cps = format!("{:.0}", note.cps);
        rows = rows.child(
            div()
                .id(format!("cue-row-{index}"))
                .h(px(ROW))
                .px(px(16.))
                .flex()
                .items_center()
                .gap(px(10.))
                .cursor_pointer()
                .bg(if on { theme.raised } else { theme.stage })
                .opacity(if dim { 0.35 } else { 1.0 })
                .hover(|style| style.bg(theme.raised))
                .on_click(move |_event, _window, cx| {
                    app.update(cx, |this, cx| this.seek_cue(index, cx)).ok();
                })
                .child(div().w(px(3.)).h(px(18.)).rounded_full().bg(speaker_color(&name)).flex_none())
                .child(mono_cell(36.0, format!("{:02}", cue.index), theme.faint))
                .child(mono_cell(118.0, srt::srt_stamp(cue.start_ms), theme.muted))
                .child(mono_cell(118.0, srt::srt_stamp(cue.end_ms), theme.muted))
                .child(mono_cell(40.0, cps, if note.fast || note.short { theme.warn } else { theme.muted }))
                .child(
                    div()
                        .w(px(92.))
                        .flex_none()
                        .truncate()
                        .text_size(px(11.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(speaker_color(&name))
                        .child(if name.is_empty() { "—".to_string() } else { name }),
                )
                .child(div().flex_1().min_w_0().truncate().child(cue.plain())),
        );
    }

    div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .child(sheet_header(theme))
        .child(rows)
}

fn sheet_header(theme: Theme) -> gpui::Div {
    div()
        .h(px(28.))
        .px(px(16.))
        .flex()
        .items_center()
        .gap(px(10.))
        .text_size(px(10.))
        .text_color(theme.faint)
        .letter_spacing(px(0.6))
        .border_b_1()
        .border_color(theme.line)
        .child(div().w(px(3.)).flex_none())
        .child(header_cell(36.0, "#"))
        .child(header_cell(118.0, "IN"))
        .child(header_cell(118.0, "OUT"))
        .child(header_cell(40.0, "CPS"))
        .child(header_cell(92.0, "SPEAKER"))
        .child(div().flex_1().child("CAPTION"))
}

fn cue_panel(recording: Option<&Recording>, index: Option<usize>, theme: Theme) -> Stateful<gpui::Div> {
    let Some(recording) = recording else {
        return quiet(theme, "Open a recording to inspect a cue.");
    };
    let Some(index) = index else {
        return quiet(theme, "This recording has no cues yet.");
    };
    let Some(cue) = recording.cues.get(index) else {
        return quiet(theme, "This recording has no cues yet.");
    };
    let note = srt::assess(&recording.cues, index);
    let warnings = srt::warnings(&recording.cues, index);
    let ratio = (note.cps / 30.0).clamp(0.0, 1.0) as f32;
    let mut panel = div()
        .id("inspector-scroll")
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .px(px(16.))
        .pb(px(20.))
        .flex()
        .flex_col()
        .gap(px(14.))
        .child(section_label(theme, "CUE"))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(stat(theme, "In", srt::srt_stamp(cue.start_ms)))
                .child(stat(theme, "Out", srt::srt_stamp(cue.end_ms)))
                .child(stat(theme, "Duration", format!("{:.2}s", cue.duration_ms() as f32 / 1000.0)))
                .child(stat(theme, "Characters", note.chars.to_string()))
                .child(stat(theme, "Longest line", format!("{} / 42", note.max_line)))
                .child(stat(
                    theme,
                    "Gap before",
                    note.gap_before_ms
                        .map(|ms| format!("{:.2}s", ms as f32 / 1000.0))
                        .unwrap_or_else(|| "—".into()),
                ))
                .child(stat(
                    theme,
                    "Gap after",
                    note.gap_after_ms
                        .map(|ms| format!("{:.2}s", ms as f32 / 1000.0))
                        .unwrap_or_else(|| "overlap".into()),
                )),
        )
        .child(section_label(theme, "READING SPEED"))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(
                    div()
                        .relative()
                        .h(px(6.))
                        .w_full()
                        .rounded_full()
                        .bg(theme.raised)
                        .child(
                            div()
                                .absolute()
                                .left(px(0.))
                                .top(px(0.))
                                .bottom(px(0.))
                                .w(relative(ratio))
                                .rounded_full()
                                .bg(if note.fast { theme.warn } else { theme.bone }),
                        )
                        .child(
                            div()
                                .absolute()
                                .top(px(-3.))
                                .h(px(12.))
                                .w(px(1.))
                                .left(relative(21.0 / 30.0))
                                .bg(theme.faint),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .justify_between()
                        .font_family("JetBrains Mono")
                        .text_size(px(10.))
                        .text_color(theme.faint)
                        .child("0")
                        .child("21")
                        .child("30 cps"),
                )
                .child(
                    div()
                        .font_family("JetBrains Mono")
                        .text_size(px(18.))
                        .text_color(if note.fast { theme.warn } else { theme.text })
                        .child(format!("{:.0}", note.cps)),
                ),
        )
        .child(section_label(theme, "NOTES"));
    if warnings.is_empty() {
        panel = panel.child(div().text_size(px(12.)).text_color(theme.muted).child("Within the usual caption limits."));
    } else {
        for (nth, warning) in warnings.iter().enumerate() {
            panel = panel.child(
                div()
                    .id(format!("note-{nth}"))
                    .flex()
                    .gap(px(8.))
                    .child(div().w(px(2.)).rounded_full().bg(theme.warn).flex_none())
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .whitespace_normal()
                            .text_size(px(12.))
                            .text_color(theme.text)
                            .child(warning.clone()),
                    ),
            );
        }
    }
    panel = panel.child(section_label(theme, "SRT")).child(srt_source(cue, theme)).child(
        div()
            .text_size(px(11.))
            .text_color(theme.faint)
            .child("21 characters a second · 42 a line · two lines · one second on screen."),
    );
    panel
}

fn file_panel(
    recording: Option<&Recording>,
    log: &[String],
    busy: bool,
    app: gpui::WeakEntity<Studio>,
    theme: Theme,
) -> Stateful<gpui::Div> {
    let Some(recording) = recording else {
        return quiet(theme, "Open a recording to see its files.");
    };
    let source = recording
        .source
        .as_ref()
        .map(|path| crate::library::pretty_path(path))
        .unwrap_or_else(|| "No audio file".into());
    let srt = recording
        .srt_path
        .as_ref()
        .map(|path| crate::library::pretty_path(path))
        .unwrap_or_else(|| "No SRT yet".into());
    let mut panel = div()
        .id("file-scroll")
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .px(px(16.))
        .pb(px(20.))
        .flex()
        .flex_col()
        .gap(px(12.))
        .child(section_label(theme, "AUDIO"))
        .child(path_block(&source, theme))
        .child(section_label(theme, "SRT"))
        .child(path_block(&srt, theme))
        .child(
            div()
                .text_size(px(12.))
                .text_color(theme.muted)
                .child(format!(
                    "{} cues · {}",
                    recording.cues.len(),
                    srt::clock(recording.duration_ms)
                )),
        );
    if recording.source.is_some() {
        let app = app.clone();
        let label = if busy { "Transcribing…" } else { "Transcribe locally" };
        panel = panel.child(
            div()
                .id("transcribe")
                .mt(px(8.))
                .h(px(32.))
                .px(px(12.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .bg(theme.bone)
                .text_color(theme.ink)
                .text_size(px(12.))
                .font_weight(FontWeight::SEMIBOLD)
                .cursor_pointer()
                .child(label)
                .on_click(move |_event, _window, cx| {
                    app.update(cx, |this, cx| this.transcribe(cx)).ok();
                }),
        );
    }
    if !log.is_empty() {
        let mut well = div()
            .p(px(10.))
            .rounded(px(8.))
            .bg(theme.stage)
            .font_family("JetBrains Mono")
            .text_size(px(11.))
            .text_color(theme.muted)
            .flex()
            .flex_col()
            .gap(px(4.));
        for line in log {
            well = well.child(div().child(line.clone()));
        }
        panel = panel.child(well);
    }
    panel
}

fn srt_source(cue: &Cue, theme: Theme) -> gpui::Div {
    let mut well = div()
        .p(px(12.))
        .rounded(px(8.))
        .bg(theme.stage)
        .font_family("JetBrains Mono")
        .text_size(px(12.))
        .line_height(px(18.))
        .text_color(theme.muted)
        .flex()
        .flex_col();
    for line in [
        cue.index.to_string(),
        format!("{} --> {}", srt::srt_stamp(cue.start_ms), srt::srt_stamp(cue.end_ms)),
    ] {
        well = well.child(div().child(line));
    }
    for line in cue.raw.lines() {
        well = well.child(div().child(line.to_string()));
    }
    well
}

fn sidebar_footer(app: gpui::WeakEntity<Studio>, theme: Theme, toast: Option<String>) -> gpui::Div {
    let mut footer = div()
        .p(px(12.))
        .border_t_1()
        .border_color(theme.line)
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(
            div()
                .id("import-srt")
                .h(px(32.))
                .w_full()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .bg(theme.bone)
                .text_color(theme.ink)
                .text_size(px(12.))
                .font_weight(FontWeight::SEMIBOLD)
                .cursor_pointer()
                .child("Import SRT")
                .on_click(move |_event, _window, cx| {
                    app.update(cx, |this, cx| this.import_srt(cx)).ok();
                }),
        );
    if let Some(toast) = toast {
        footer = footer.child(
            div()
                .text_size(px(11.))
                .text_color(theme.muted)
                .line_clamp(3)
                .child(toast),
        );
    }
    footer
}

fn transport_button(
    id: &'static str,
    label: &'static str,
    app: gpui::WeakEntity<Studio>,
    theme: Theme,
    action: fn(&mut Studio, &mut Context<Studio>),
) -> Stateful<gpui::Div> {
    let primary = label == "Play" || label == "Pause";
    let button = div()
        .id(id)
        .h(px(28.))
        .px(px(12.))
        .min_w(px(64.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.))
        .cursor_pointer()
        .text_size(px(12.))
        .font_weight(FontWeight::SEMIBOLD)
        .when(primary, |button| button.bg(theme.bone).text_color(theme.ink))
        .when(!primary, |button| {
            button
                .border_1()
                .border_color(theme.line)
                .text_color(theme.muted)
                .hover(|style| style.bg(theme.raised).text_color(theme.text))
        })
        .child(label)
        .on_click(move |_event, _window, cx| {
            app.update(cx, |this, cx| action(this, cx)).ok();
        });
    button
}

fn chip(id: &'static str, label: &'static str, on: bool, theme: Theme) -> Stateful<gpui::Div> {
    div()
        .id(id)
        .h(px(26.))
        .px(px(8.))
        .flex()
        .items_center()
        .rounded(px(6.))
        .cursor_pointer()
        .text_size(px(12.))
        .when(on, |chip| chip.bg(theme.bone).text_color(theme.ink).font_weight(FontWeight::SEMIBOLD))
        .when(!on, |chip| {
            chip.text_color(theme.muted)
                .hover(|style| style.bg(theme.raised).text_color(theme.text))
        })
        .child(label)
}

fn mark(theme: Theme) -> gpui::Div {
    div()
        .relative()
        .w(px(16.))
        .h(px(16.))
        .rounded(px(3.))
        .border_1()
        .border_color(theme.bone)
        .child(div().absolute().left(px(7.)).top(px(3.)).w(px(1.)).h(px(10.)).bg(theme.bone))
        .child(div().absolute().top(px(7.)).left(px(3.)).h(px(1.)).w(px(10.)).bg(theme.bone))
}

fn corner(left: bool, top: bool) -> gpui::Div {
    let color = rgb_quiet();
    let mut mark = div().absolute().w(px(12.)).h(px(12.));
    mark = if left { mark.left(px(12.)) } else { mark.right(px(12.)) };
    mark = if top { mark.top(px(12.)) } else { mark.bottom(px(12.)) };
    let horizontal = if top {
        div().absolute().top(px(0.)).h(px(1.)).w(px(12.)).bg(color)
    } else {
        div().absolute().bottom(px(0.)).h(px(1.)).w(px(12.)).bg(color)
    };
    let horizontal = if left { horizontal.left(px(0.)) } else { horizontal.right(px(0.)) };
    let vertical = if left {
        div().absolute().left(px(0.)).w(px(1.)).h(px(12.)).bg(color)
    } else {
        div().absolute().right(px(0.)).w(px(1.)).h(px(12.)).bg(color)
    };
    let vertical = if top { vertical.top(px(0.)) } else { vertical.bottom(px(0.)) };
    mark.child(horizontal).child(vertical)
}

fn mono_cell(width: f32, text: String, color: gpui::Rgba) -> gpui::Div {
    div()
        .w(px(width))
        .flex_none()
        .font_family("JetBrains Mono")
        .text_size(px(12.))
        .text_color(color)
        .truncate()
        .child(text)
}

fn header_cell(width: f32, text: &'static str) -> gpui::Div {
    div().w(px(width)).flex_none().child(text)
}

fn section_label(theme: Theme, text: &'static str) -> gpui::Div {
    div()
        .text_size(px(10.))
        .text_color(theme.faint)
        .font_weight(FontWeight::SEMIBOLD)
        .letter_spacing(px(1.0))
        .child(text)
}

fn stat(theme: Theme, label: &'static str, value: String) -> gpui::Div {
    div()
        .flex()
        .justify_between()
        .items_center()
        .child(div().text_size(px(12.)).text_color(theme.faint).child(label))
        .child(
            div()
                .font_family("JetBrains Mono")
                .text_size(px(12.))
                .child(value),
        )
}

fn path_block(text: &str, theme: Theme) -> gpui::Div {
    div()
        .w_full()
        .text_size(px(12.))
        .text_color(theme.muted)
        .font_family("JetBrains Mono")
        .whitespace_normal()
        .line_clamp(3)
        .child(text.to_string())
}

fn quiet(theme: Theme, text: &'static str) -> Stateful<gpui::Div> {
    div()
        .id("quiet-note")
        .px(px(16.))
        .text_size(px(12.))
        .text_color(theme.muted)
        .child(text)
}

fn library_meta(recording: &Recording, issues: usize) -> String {
    let mut text = if recording.cues.is_empty() {
        "No captions".to_string()
    } else {
        format!("{} cues", recording.cues.len())
    };
    if recording.sample {
        text = format!("{text} · sample");
    }
    if issues > 0 {
        text = format!("{text} · {issues} to review");
    }
    text
}

fn waveform_bars(peaks: &[f32], origin: f64, span: f64, duration: f64, count: usize) -> Vec<f32> {
    if peaks.is_empty() || duration <= 0.0 || span <= 0.0 || count == 0 {
        return vec![0.08; count.max(1)];
    }
    let mut bars = Vec::with_capacity(count);
    for index in 0..count {
        let start = origin + span * (index as f64 / count as f64);
        let end = origin + span * ((index + 1) as f64 / count as f64);
        let from = ((start / duration) * peaks.len() as f64) as usize;
        let to = (((end / duration) * peaks.len() as f64) as usize).max(from + 1);
        let to = to.min(peaks.len());
        let from = from.min(peaks.len().saturating_sub(1));
        let peak = peaks[from..to].iter().copied().fold(0.0, f32::max);
        bars.push(peak.clamp(0.04, 1.0));
    }
    bars
}

fn ruler_step(span: f64) -> f64 {
    if span <= 15_000.0 {
        2_000.0
    } else if span <= 45_000.0 {
        5_000.0
    } else if span <= 120_000.0 {
        15_000.0
    } else if span <= 300_000.0 {
        30_000.0
    } else {
        60_000.0
    }
}

fn rgb_quiet() -> gpui::Rgba {
    gpui::rgb(0x5C5C5C)
}

fn rgb_bezel() -> gpui::Rgba {
    gpui::rgb(0x08090B)
}
