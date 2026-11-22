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

use crate::config::{self, PluginSettings};
use crate::library::{self, Recording};
use crate::okf::Concept;
use crate::plugins::{self, FallacyHit, Profile};
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Fit {
    Desk,
    Tablet,
    Phone,
    Watch,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    Session,
    Library,
    Cue,
    Package,
    Models,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pane {
    Cue,
    File,
    Package,
    Fallacies,
    Profile,
}

fn fit_for(width: f32, height: f32) -> Fit {
    if width < 300.0 || (width < 420.0 && height < 280.0) {
        Fit::Watch
    } else if width < 700.0 {
        Fit::Phone
    } else if width < 1180.0 {
        Fit::Tablet
    } else {
        Fit::Desk
    }
}

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
    pane: Pane,
    screen: Screen,
    concept: Option<String>,
    query: Entity<EditableTextState>,
    whisper: Entity<EditableTextState>,
    diarize: Entity<EditableTextState>,
    token: Entity<EditableTextState>,
    token_set: bool,
    fallacies_on: bool,
    dsm_on: bool,
    laya_path: Entity<EditableTextState>,
    laya_command: String,
    bonsai_path: Entity<EditableTextState>,
    bonsai_endpoint: Entity<EditableTextState>,
    laya_status: String,
    bonsai_status: String,
    fallacy_hits: Vec<FallacyHit>,
    profiles: Vec<Profile>,
    profile_speaker: Option<String>,
    plugin_note: Option<String>,
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
        let models = config::load();
        let plugins = config::load_plugins();
        let query = cx.new(|cx| EditableTextState::new(StringStorage::default(), cx));
        let whisper = cx.new(|cx| EditableTextState::new(StringStorage::from(models.whisper_path.clone()), cx));
        let diarize = cx.new(|cx| EditableTextState::new(StringStorage::from(models.diarization_id.clone()), cx));
        let token = cx.new(|cx| EditableTextState::new(StringStorage::default(), cx));
        let laya_path = cx.new(|cx| EditableTextState::new(StringStorage::from(plugins.laya_path.clone()), cx));
        let bonsai_path = cx.new(|cx| EditableTextState::new(StringStorage::from(plugins.bonsai_path.clone()), cx));
        let bonsai_endpoint = cx.new(|cx| EditableTextState::new(StringStorage::from(plugins.bonsai_endpoint.clone()), cx));
        let subscription = cx.subscribe(&query, |_, _, _: &TextChanged, cx| cx.notify());
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let recordings = library::load_library();
        let selected = recordings.first().map(|recording| recording.id.clone());
        let fallacy_hits = selected
            .as_ref()
            .and_then(|id| recordings.iter().find(|recording| &recording.id == id))
            .map(|recording| plugins::read_hits(&recording.dir))
            .unwrap_or_default();
        let profiles = plugins::read_profiles();
        let profile_speaker = profiles.first().map(|profile| profile.speaker.clone());
        let laya_status = plugins::laya_status(&plugins.laya_path);
        let bonsai_status = plugins::bonsai_status(&plugins.bonsai_path);
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
            pane: Pane::Cue,
            screen: Screen::Session,
            concept: None,
            query,
            whisper,
            diarize,
            token,
            token_set: models.token_set,
            fallacies_on: plugins.fallacies,
            dsm_on: plugins.dsm,
            laya_path,
            laya_command: plugins.laya_command,
            bonsai_path,
            bonsai_endpoint,
            laya_status,
            bonsai_status,
            fallacy_hits,
            profiles,
            profile_speaker,
            plugin_note: None,
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

    fn editing(&self, window: &Window, cx: &App) -> bool {
        [
            &self.query,
            &self.whisper,
            &self.diarize,
            &self.token,
            &self.laya_path,
            &self.bonsai_path,
            &self.bonsai_endpoint,
        ]
            .iter()
            .any(|field| field.read(cx).focus_handle(cx).is_focused(window))
    }

    fn toggle_play(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing(window, cx) || self.duration() <= 0.0 {
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
        self.screen = Screen::Session;
        self.concept = None;
        self.origin = 0.0;
        self.playhead = self
            .current()
            .and_then(|recording| recording.cues.first().map(|cue| cue.start_ms as f64))
            .unwrap_or(0.0);
        self.sheet.scroll_to_top_of_item(0);
        self.fallacy_hits = self
            .current()
            .map(|recording| plugins::read_hits(&recording.dir))
            .unwrap_or_default();
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

        let mut shell = div()
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
                if !this.editing(window, cx) {
                    this.step_cue(-1, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &NextCue, window, cx| {
                if !this.editing(window, cx) {
                    this.step_cue(1, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &PreviousRecording, window, cx| {
                if !this.editing(window, cx) {
                    this.step_recording(-1, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &NextRecording, window, cx| {
                if !this.editing(window, cx) {
                    this.step_recording(1, cx);
                }
            }));

        let width = window.viewport_size().width.as_f32();
        let fit = fit_for(width, window.viewport_size().height.as_f32());
        match fit {
            Fit::Watch => shell = shell.child(self.watch(app, theme)),
            Fit::Phone => shell = shell.child(self.phone(&query, app, theme, cx)),
            Fit::Tablet if width < 980.0 => shell = shell.child(self.tablet(&query, app, theme, cx)),
            Fit::Tablet => {
                let inspector = if self.screen == Screen::Models { None } else { Some(280.0) };
                shell = shell.child(self.columns(&query, app, theme, cx, 200.0, inspector, true));
            }
            Fit::Desk => {
                let inspector = if self.screen == Screen::Models { None } else { Some(320.0) };
                shell = shell.child(self.columns(&query, app, theme, cx, 260.0, inspector, false));
            }
        }
        shell
    }
}

impl Studio {
    fn columns(
        &self,
        query: &str,
        app: gpui::WeakEntity<Self>,
        theme: Theme,
        cx: &mut Context<Self>,
        sidebar: f32,
        inspector: Option<f32>,
        slim: bool,
    ) -> gpui::Div {
        let main = if self.screen == Screen::Models {
            self.models_page(app.clone(), theme, cx)
        } else {
            self.stage(query, app.clone(), theme, cx, slim)
        };
        let mut row = div()
            .size_full()
            .flex()
            .child(self.sidebar(query, app.clone(), theme, cx, sidebar))
            .child(main);
        if let Some(width) = inspector {
            row = row.child(self.inspector(app, theme, Some(width)));
        }
        row
    }

    fn tablet(
        &self,
        query: &str,
        app: gpui::WeakEntity<Self>,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let frame = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .child(self.section_switch(app.clone(), theme, false));
        let frame = match self.screen {
            Screen::Cue => frame.child(self.inspector(app.clone(), theme, None)),
            Screen::Package => frame.child(package_page(self.current(), self.concept.as_deref(), app.clone(), theme)),
            Screen::Models => frame.child(self.models_page(app.clone(), theme, cx)),
            _ => frame.child(self.stage(query, app.clone(), theme, cx, true)),
        };
        div()
            .size_full()
            .flex()
            .child(self.sidebar(query, app, theme, cx, 188.0))
            .child(frame)
    }

    fn phone(
        &self,
        query: &str,
        app: gpui::WeakEntity<Self>,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let page = div().flex_1().min_h_0().min_w_0();
        let page = match self.screen {
            Screen::Library => page.child(self.library_page(query, app.clone(), theme, cx)),
            Screen::Cue => page.child(self.inspector(app.clone(), theme, None)),
            Screen::Package => page.child(package_page(self.current(), self.concept.as_deref(), app.clone(), theme)),
            Screen::Models => page.child(self.models_page(app.clone(), theme, cx)),
            Screen::Session => page.child(self.stage(query, app.clone(), theme, cx, true)),
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(page)
            .child(self.bottom_nav(app, theme))
    }

    fn watch(&self, app: gpui::WeakEntity<Self>, theme: Theme) -> gpui::Div {
        if self.screen == Screen::Library {
            return self.watch_library(app, theme);
        }
        let recording = self.current();
        let title = recording.map(|recording| recording.title.clone()).unwrap_or_else(|| "Socioscope".into());
        let caption = recording
            .and_then(|recording| {
                let index = self.inspected()?;
                recording.cues.get(index).map(|cue| cue.plain())
            })
            .unwrap_or_else(|| "No caption".into());
        let play = app.clone();
        let library = app.clone();
        let next = app.clone();
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.bg)
            .p(px(8.))
            .gap(px(6.))
            .child(
                div()
                    .text_size(px(12.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .truncate()
                    .child(title),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .text_size(px(13.))
                    .whitespace_normal()
                    .line_clamp(5)
                    .child(caption),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(transport_button(
                        "watch-play",
                        if self.playing { "Pause" } else { "Play" },
                        play,
                        theme,
                        |this, cx| {
                            this.playing = !this.playing;
                            this.last_tick = Instant::now();
                            cx.notify();
                        },
                    ))
                    .child(
                        div()
                            .font_family("JetBrains Mono")
                            .text_size(px(11.))
                            .child(srt::transport_clock(self.playhead)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(6.))
                    .child(text_button("watch-library", "Library", library, theme, |this, cx| {
                        this.screen = Screen::Library;
                        cx.notify();
                    }))
                    .child(text_button("watch-next", "Next", next, theme, |this, cx| {
                        this.step_cue(1, cx);
                    })),
            )
    }

    fn watch_library(&self, app: gpui::WeakEntity<Self>, theme: Theme) -> gpui::Div {
        let mut list = div().id("watch-list").flex_1().min_h_0().overflow_y_scroll().flex().flex_col().gap(px(2.));
        for recording in &self.recordings {
            let id = recording.id.clone();
            let app = app.clone();
            let on = self.selected.as_deref() == Some(recording.id.as_str());
            list = list.child(
                div()
                    .id(format!("watch-{id}"))
                    .px(px(6.))
                    .py(px(4.))
                    .rounded(px(4.))
                    .cursor_pointer()
                    .text_size(px(12.))
                    .truncate()
                    .bg(if on { theme.raised } else { theme.bg })
                    .child(recording.title.clone())
                    .on_click(move |_event, _window, cx| {
                        app.update(cx, |this, cx| this.choose(&id, cx)).ok();
                    }),
            );
        }
        let back = app.clone();
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.bg)
            .p(px(8.))
            .gap(px(6.))
            .child(div().text_size(px(10.)).text_color(theme.faint).child("LIBRARY"))
            .child(list)
            .child(text_button("watch-back", "Back", back, theme, |this, cx| {
                this.screen = Screen::Session;
                cx.notify();
            }))
    }

    fn library_page(
        &self,
        query: &str,
        app: gpui::WeakEntity<Self>,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        self.sidebar(query, app, theme, cx, 10_000.0)
            .w_full()
            .flex_1()
            .min_h_0()
            .border_r_0()
    }

    fn bottom_nav(&self, app: gpui::WeakEntity<Self>, theme: Theme) -> gpui::Div {
        div()
            .h(px(48.))
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .px(px(6.))
            .bg(theme.bg)
            .border_t_1()
            .border_color(theme.line)
            .child(self.nav_item("nav-library", "Library", Screen::Library, app.clone(), theme))
            .child(self.nav_item("nav-play", "Play", Screen::Session, app.clone(), theme))
            .child(self.nav_item("nav-cue", "Cue", Screen::Cue, app.clone(), theme))
            .child(self.nav_item("nav-pack", "Pack", Screen::Package, app.clone(), theme))
            .child(self.nav_item("nav-models", "Models", Screen::Models, app, theme))
    }

    fn nav_item(
        &self,
        id: &'static str,
        label: &'static str,
        screen: Screen,
        app: gpui::WeakEntity<Self>,
        theme: Theme,
    ) -> Stateful<gpui::Div> {
        let on = self.screen == screen;
        div()
            .id(id)
            .flex_1()
            .h(px(36.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.))
            .cursor_pointer()
            .text_size(px(11.))
            .font_weight(if on { FontWeight::SEMIBOLD } else { FontWeight::NORMAL })
            .text_color(if on { theme.ink } else { theme.muted })
            .bg(if on { theme.bone } else { theme.bg })
            .child(label)
            .on_click(move |_event, _window, cx| {
                app.update(cx, |this, cx| {
                    this.screen = screen;
                    cx.notify();
                })
                .ok();
            })
    }

    fn section_switch(&self, app: gpui::WeakEntity<Self>, theme: Theme, include_library: bool) -> gpui::Div {
        let mut row = div()
            .px(px(12.))
            .pt(px(8.))
            .pb(px(4.))
            .flex()
            .gap(px(6.))
            .flex_none();
        if include_library {
            row = row.child(self.nav_chip("switch-library", "Library", Screen::Library, app.clone(), theme));
        }
        row.child(self.nav_chip("switch-session", "Session", Screen::Session, app.clone(), theme))
            .child(self.nav_chip("switch-cue", "Cue", Screen::Cue, app.clone(), theme))
            .child(self.nav_chip("switch-package", "Package", Screen::Package, app.clone(), theme))
            .child(self.nav_chip("switch-models", "Models", Screen::Models, app, theme))
    }

    fn nav_chip(
        &self,
        id: &'static str,
        label: &'static str,
        screen: Screen,
        app: gpui::WeakEntity<Self>,
        theme: Theme,
    ) -> Stateful<gpui::Div> {
        let on = self.screen == screen || (screen == Screen::Session && self.screen == Screen::Library);
        chip(id, label, on, theme).on_click(move |_event, _window, cx| {
            app.update(cx, |this, cx| {
                this.screen = screen;
                cx.notify();
            })
            .ok();
        })
    }

    fn sidebar(
        &self,
        query: &str,
        app: gpui::WeakEntity<Self>,
        theme: Theme,
        cx: &mut Context<Self>,
        width: f32,
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
            .w(px(width))
            .max_w_full()
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
            .child(sidebar_footer(import, app, theme, self.toast.clone(), self.screen == Screen::Models))
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
        slim: bool,
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
            .child(stage_header(recording, theme, slim))
            .child(monitor(recording, inspected, self.playhead, theme, if slim { 176.0 } else { 228.0 }))
            .child(self.transport(app.clone(), theme, slim))
            .child(timeline(
                recording,
                origin,
                span,
                self.playhead,
                self.speaker.as_deref(),
                self.picked,
                app.clone(),
                theme,
                slim,
            ))
            .child(cue_sheet(
                recording,
                active,
                self.picked,
                self.speaker.as_deref(),
                &self.sheet,
                app,
                theme,
                slim,
            ))
    }

    fn transport(&self, app: gpui::WeakEntity<Self>, theme: Theme, slim: bool) -> gpui::Div {
        let duration = self.duration();
        let play = app.clone();
        let previous = app.clone();
        let next = app.clone();
        let follow = app.clone();
        let follow_on = self.follow;
        let mut row = div()
            .min_h(px(if slim { 40.0 } else { 48.0 }))
            .px(px(if slim { 12.0 } else { 20.0 }))
            .py(px(6.))
            .flex()
            .flex_wrap()
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
            .child(div().flex_1().min_w(px(8.)));

        if slim {
            return row;
        }

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

    fn inspector(&self, app: gpui::WeakEntity<Self>, theme: Theme, width: Option<f32>) -> gpui::Div {
        let recording = self.current().cloned();
        let mut panel = div().h_full().flex().flex_col().bg(theme.bg);
        panel = if let Some(width) = width {
            panel.w(px(width)).flex_none().border_l_1().border_color(theme.line)
        } else {
            panel.flex_1().min_w_0().min_h_0()
        };
        let cue_tab = app.clone();
        let file_tab = app.clone();
        let package_tab = app.clone();
        let fallacy_tab = app.clone();
        let profile_tab = app.clone();
        let mut tabs = div()
            .px(px(12.))
            .pt(px(12.))
            .pb(px(8.))
            .flex()
            .flex_wrap()
            .gap(px(6.))
            .child(chip("tab-cue", "Cue", self.pane == Pane::Cue, theme).on_click(move |_e, _w, cx| {
                cue_tab.update(cx, |this, cx| {
                    this.pane = Pane::Cue;
                    cx.notify();
                }).ok();
            }))
            .child(chip("tab-file", "File", self.pane == Pane::File, theme).on_click(move |_e, _w, cx| {
                file_tab.update(cx, |this, cx| {
                    this.pane = Pane::File;
                    cx.notify();
                }).ok();
            }))
            .child(chip("tab-package", "Package", self.pane == Pane::Package, theme).on_click(move |_e, _w, cx| {
                package_tab.update(cx, |this, cx| {
                    this.pane = Pane::Package;
                    cx.notify();
                }).ok();
            }));
        if self.fallacies_on {
            tabs = tabs.child(chip("tab-fallacies", "Fallacies", self.pane == Pane::Fallacies, theme).on_click(move |_e, _w, cx| {
                fallacy_tab.update(cx, |this, cx| {
                    this.pane = Pane::Fallacies;
                    cx.notify();
                }).ok();
            }));
        }
        if self.dsm_on {
            tabs = tabs.child(chip("tab-profile", "Profile", self.pane == Pane::Profile, theme).on_click(move |_e, _w, cx| {
                profile_tab.update(cx, |this, cx| {
                    this.pane = Pane::Profile;
                    cx.notify();
                }).ok();
            }));
        }
        panel = panel.child(tabs);
        let pane = match self.pane {
            Pane::Fallacies if !self.fallacies_on => Pane::Cue,
            Pane::Profile if !self.dsm_on => Pane::Cue,
            other => other,
        };
        match pane {
            Pane::File => panel.child(file_panel(recording.as_ref(), &self.log, self.busy, app, theme)),
            Pane::Package => panel.child(package_page(recording.as_ref(), self.concept.as_deref(), app, theme)),
            Pane::Fallacies => panel.child(self.fallacy_pane(app, theme)),
            Pane::Profile => panel.child(self.profile_pane(app, theme)),
            Pane::Cue => panel.child(cue_panel(recording.as_ref(), self.inspected(), theme)),
        }
    }

    fn models_page(&self, app: gpui::WeakEntity<Self>, theme: Theme, cx: &mut Context<Self>) -> gpui::Div {
        let back = app.clone();
        let save = app.clone();
        let whisper = self.whisper.read(cx).as_str().to_string();
        let diarize = self.diarize.read(cx).as_str().to_string();
        let token_line = if self.token_set {
            "A Hugging Face token is saved. Paste a new one only to replace it."
        } else {
            "No Hugging Face token yet. Diarization needs one to download the model."
        };
        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(theme.stage)
            .child(
                div()
                    .px(px(20.))
                    .pt(px(16.))
                    .pb(px(8.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().text_size(px(20.)).font_weight(FontWeight::SEMIBOLD).child("Config"))
                    .child(text_button("config-back", "Session", back, theme, |this, cx| {
                        this.screen = Screen::Session;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .id("config-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(px(20.))
                    .pb(px(24.))
                    .flex()
                    .flex_col()
                    .gap(px(16.))
                    .child(div().text_size(px(12.)).text_color(theme.muted).whitespace_normal().child(
                        "The studio and the local transcriber share config.yaml.",
                    ))
                    .child(section_label(theme, "MODELS"))
                    .child(model_card(
                        theme,
                        "Transcription",
                        "whisper.cpp",
                        "model-whisper",
                        &self.whisper,
                        "Path to a ggml model",
                        &config::whisper_status(&whisper),
                    ))
                    .child(model_card(
                        theme,
                        "Diarization",
                        "pyannote.audio",
                        "model-diarize",
                        &self.diarize,
                        "pyannote/speaker-diarization-3.1",
                        if diarize.trim().is_empty() { "Model id is empty" } else { "Model id" },
                    ))
                    .child(section_label(theme, "ACCESS"))
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme.muted)
                            .whitespace_normal()
                            .child(token_line),
                    )
                    .child(model_field(theme, "model-token", &self.token, "Paste a token to replace the saved one"))
                    .child(section_label(theme, "PLUGINS"))
                    .child(self.plugin_toggle(app.clone(), theme, "plug-fallacies", "Fallacies", "One session. Laya scores each turn.", self.fallacies_on))
                    .child(self.plugin_toggle(app.clone(), theme, "plug-dsm", "DSM-5", "Profiles across conversations. Evidence, not a diagnosis.", self.dsm_on))
                    .child(section_label(theme, "ENGINES"))
                    .child(model_card(
                        theme,
                        "Laya",
                        "Decision model for both plugins",
                        "model-laya",
                        &self.laya_path,
                        "Path to laya-f16.gguf",
                        &self.laya_status,
                    ))
                    .child(model_card(
                        theme,
                        "Bonsai",
                        "Optional correlation for profiles",
                        "model-bonsai",
                        &self.bonsai_path,
                        "Path to a Bonsai GGUF",
                        &self.bonsai_status,
                    ))
                    .child(model_field(theme, "model-bonsai-endpoint", &self.bonsai_endpoint, "http://127.0.0.1:8080"))
                    .child(
                        div()
                            .id("save-config")
                            .mt(px(4.))
                            .h(px(32.))
                            .px(px(14.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.))
                            .bg(theme.bone)
                            .text_color(theme.ink)
                            .text_size(px(12.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .cursor_pointer()
                            .child("Save config")
                            .on_click(move |_event, _window, cx| {
                                save.update(cx, |this, cx| this.save_models(cx)).ok();
                            }),
                    ),
            )
    }

    fn save_models(&mut self, cx: &mut Context<Self>) {
        self.persist(cx, true);
    }

    fn plugin_settings(&self, cx: &App) -> PluginSettings {
        PluginSettings {
            fallacies: self.fallacies_on,
            dsm: self.dsm_on,
            laya_path: self.laya_path.read(cx).as_str().to_string(),
            laya_command: self.laya_command.clone(),
            bonsai_path: self.bonsai_path.read(cx).as_str().to_string(),
            bonsai_endpoint: self.bonsai_endpoint.read(cx).as_str().to_string(),
        }
    }

    fn persist(&mut self, cx: &mut Context<Self>, clear_token: bool) {
        let whisper = self.whisper.read(cx).as_str().to_string();
        let diarize = self.diarize.read(cx).as_str().to_string();
        let token = if clear_token { self.token.read(cx).as_str().to_string() } else { String::new() };
        let plugins = self.plugin_settings(cx);
        match config::save_studio(&whisper, &diarize, &token, &plugins) {
            Ok(()) => {
                let models = config::load();
                self.token_set = models.token_set;
                if clear_token {
                    self.token.update(cx, |field, cx| field.emplace("", cx));
                }
                self.laya_status = plugins::laya_status(&plugins.laya_path);
                self.bonsai_status = plugins::bonsai_status(&plugins.bonsai_path);
                self.toast = Some("Saved config.yaml".into());
            }
            Err(err) => self.toast = Some(err),
        }
        cx.notify();
    }

    fn toggle_plugin(&mut self, dsm: bool, cx: &mut Context<Self>) {
        if dsm {
            self.dsm_on = !self.dsm_on;
            if !self.dsm_on && self.pane == Pane::Profile {
                self.pane = Pane::Cue;
            }
        } else {
            self.fallacies_on = !self.fallacies_on;
            if !self.fallacies_on && self.pane == Pane::Fallacies {
                self.pane = Pane::Cue;
            }
        }
        self.persist(cx, false);
    }

    fn run_fallacies(&mut self, cx: &mut Context<Self>) {
        if !self.fallacies_on {
            self.toast = Some("Fallacies plugin is off.".into());
            cx.notify();
            return;
        }
        let Some(recording) = self.current().cloned() else {
            self.toast = Some("Open a recording first.".into());
            cx.notify();
            return;
        };
        if self.busy {
            return;
        }
        let settings = self.plugin_settings(cx);
        self.busy = true;
        self.toast = Some("Reading the session with Laya…".into());
        cx.notify();
        self.job = Some(cx.spawn(async move |this, async_cx| {
            let result = async_cx
                .background_spawn(async move {
                    plugins::analyze_fallacies_live(&recording, settings.fallacies, &settings.laya_path, &settings.laya_command)
                })
                .await;
            this.update(async_cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok((note, hits)) => {
                        this.toast = Some(note.summary.clone());
                        this.plugin_note = Some(note.summary);
                        this.laya_status = plugins::laya_status(&this.laya_path.read(cx).as_str());
                        if !hits.is_empty() {
                            this.fallacy_hits = hits;
                        } else {
                            this.fallacy_hits = this.current().map(|recording| plugins::read_hits(&recording.dir)).unwrap_or_default();
                        }
                        this.reload_library();
                    }
                    Err(err) => {
                        this.toast = Some(err.clone());
                        this.plugin_note = Some(err);
                    }
                }
                cx.notify();
            })
            .ok();
        }));
    }

    fn run_profiles(&mut self, cx: &mut Context<Self>) {
        if !self.dsm_on {
            self.toast = Some("DSM-5 plugin is off.".into());
            cx.notify();
            return;
        }
        if self.busy {
            return;
        }
        let settings = self.plugin_settings(cx);
        let recordings = self.recordings.clone();
        self.busy = true;
        self.toast = Some("Building evidence profiles with Laya…".into());
        cx.notify();
        self.job = Some(cx.spawn(async move |this, async_cx| {
            let result = async_cx
                .background_spawn(async move {
                    plugins::analyze_profiles_live(
                        &recordings,
                        settings.dsm,
                        &settings.laya_path,
                        &settings.laya_command,
                        &settings.bonsai_path,
                        &settings.bonsai_endpoint,
                    )
                })
                .await;
            this.update(async_cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok((note, profiles)) => {
                        this.toast = Some(note.summary.clone());
                        this.plugin_note = Some(note.summary);
                        this.profiles = profiles;
                        if this.profile_speaker.is_none() {
                            this.profile_speaker = this.profiles.first().map(|profile| profile.speaker.clone());
                        }
                        this.reload_library();
                    }
                    Err(err) => {
                        this.toast = Some(err.clone());
                        this.plugin_note = Some(err);
                    }
                }
                cx.notify();
            })
            .ok();
        }));
    }

    fn reload_library(&mut self) {
        let selected = self.selected.clone();
        self.recordings = library::load_library();
        if let Some(id) = &selected {
            if self.recordings.iter().any(|recording| &recording.id == id) {
                self.selected = Some(id.clone());
            }
        }
        self.profiles = plugins::read_profiles();
    }

    fn plugin_toggle(
        &self,
        app: gpui::WeakEntity<Self>,
        theme: Theme,
        id: &'static str,
        title: &'static str,
        detail: &'static str,
        on: bool,
    ) -> gpui::Div {
        let dsm = id == "plug-dsm";
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(div().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).child(title))
                    .child(div().text_size(px(12.)).text_color(theme.muted).whitespace_normal().child(detail)),
            )
            .child(
                div()
                    .id(id)
                    .h(px(28.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .text_size(px(12.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .bg(if on { theme.bone } else { theme.raised })
                    .text_color(if on { theme.ink } else { theme.muted })
                    .child(if on { "On" } else { "Off" })
                    .on_click(move |_event, _window, cx| {
                        app.update(cx, |this, cx| this.toggle_plugin(dsm, cx)).ok();
                    }),
            )
    }

    fn fallacy_pane(&self, app: gpui::WeakEntity<Self>, theme: Theme) -> Stateful<gpui::Div> {
        let run = app.clone();
        let mut page = div()
            .id("fallacy-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(px(16.))
            .pb(px(20.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(section_label(theme, "FALLACIES"))
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(theme.muted)
                    .whitespace_normal()
                    .child("Laya reads each turn in this session. A hit stays a draft until someone reviews it."),
            )
            .child(
                div()
                    .font_family("JetBrains Mono")
                    .text_size(px(11.))
                    .text_color(theme.faint)
                    .whitespace_normal()
                    .child(self.plugin_note.clone().unwrap_or_else(|| self.laya_status.clone())),
            )
            .child(text_button("run-fallacies", "Run on this session", run, theme, |this, cx| {
                this.run_fallacies(cx);
            }));
        if self.fallacy_hits.is_empty() {
            page = page.child(div().text_size(px(13.)).text_color(theme.muted).whitespace_normal().child("No fallacy findings for this session yet."));
        }
        for hit in &self.fallacy_hits {
            page = page.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .child(div().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).text_color(theme.warn).child(hit.kind.clone()))
                            .child(div().text_size(px(12.)).text_color(theme.faint).child(format!("{}  {:.0}%", hit.speaker, hit.noul * 100.0))),
                    )
                    .child(div().text_size(px(13.)).whitespace_normal().child(hit.quote.clone()))
                    .child(div().text_size(px(12.)).text_color(theme.muted).whitespace_normal().child(hit.label.clone())),
            );
        }
        page
    }

    fn profile_pane(&self, app: gpui::WeakEntity<Self>, theme: Theme) -> Stateful<gpui::Div> {
        let run = app.clone();
        let chosen = self
            .profiles
            .iter()
            .find(|profile| Some(profile.speaker.as_str()) == self.profile_speaker.as_deref())
            .or_else(|| self.profiles.first());
        let mut page = div()
            .id("profile-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(px(16.))
            .pb(px(20.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(section_label(theme, "DSM-5"))
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(theme.muted)
                    .whitespace_normal()
                    .child("Laya scores evidence themes for each speaker label across the library. Bonsai can correlate those themes. This is not a diagnosis."),
            )
            .child(
                div()
                    .font_family("JetBrains Mono")
                    .text_size(px(11.))
                    .text_color(theme.faint)
                    .whitespace_normal()
                    .child(self.plugin_note.clone().unwrap_or_else(|| self.laya_status.clone())),
            )
            .child(text_button("run-profiles", "Update profiles", run, theme, |this, cx| {
                this.run_profiles(cx);
            }));
        let mut row = div().flex().flex_wrap().gap(px(6.));
        for profile in &self.profiles {
            let speaker = profile.speaker.clone();
            let on = chosen.map(|item| item.speaker.as_str()) == Some(profile.speaker.as_str());
            let app = app.clone();
            row = row.child(
                div()
                    .id(format!("profile-{}", profile.slug))
                    .h(px(26.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .text_size(px(12.))
                    .bg(if on { theme.bone } else { theme.raised })
                    .text_color(if on { theme.ink } else { theme.muted })
                    .child(profile.speaker.clone())
                    .on_click(move |_event, _window, cx| {
                        let speaker = speaker.clone();
                        app.update(cx, |this, cx| {
                            this.profile_speaker = Some(speaker);
                            cx.notify();
                        })
                        .ok();
                    }),
            );
        }
        page = page.child(row);
        if let Some(profile) = chosen {
            page = page.child(div().text_size(px(16.)).font_weight(FontWeight::SEMIBOLD).child(profile.speaker.clone()));
            for theme_score in &profile.themes {
                page = page.child(
                    div()
                        .flex()
                        .justify_between()
                        .gap(px(8.))
                        .child(div().text_size(px(13.)).child(theme_score.label.clone()))
                        .child(
                            div()
                                .font_family("JetBrains Mono")
                                .text_size(px(12.))
                                .text_color(theme.muted)
                                .child(format!("{:.2}", theme_score.score)),
                        ),
                );
            }
            page = page.child(section_label(theme, "SESSIONS"));
            for session in &profile.sessions {
                page = page.child(div().text_size(px(13.)).child(session.title.clone()));
            }
            page = page.child(section_label(theme, "CORRELATION"));
            let correlation = if profile.correlation.is_empty() {
                self.bonsai_status.clone()
            } else {
                profile.correlation.clone()
            };
            page = page.child(div().text_size(px(13.)).whitespace_normal().child(correlation));
        } else {
            page = page.child(div().text_size(px(13.)).text_color(theme.muted).whitespace_normal().child("No profiles yet. Update profiles after Laya is on disk."));
        }
        page
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

fn stage_header(recording: &Recording, theme: Theme, slim: bool) -> gpui::Div {
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
        .px(px(if slim { 14.0 } else { 24.0 }))
        .pt(px(if slim { 12.0 } else { 16.0 }))
        .pb(px(if slim { 8.0 } else { 12.0 }))
        .flex()
        .flex_col()
        .gap(px(4.))
        .child(
            div()
                .text_size(px(if slim { 16.0 } else { 20.0 }))
                .font_weight(FontWeight::SEMIBOLD)
                .truncate()
                .child(recording.title.clone()),
        )
        .child(div().text_size(px(12.)).text_color(theme.muted).child(recording.detail.clone()))
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap(px(8.))
                .text_size(px(12.))
                .child(div().text_color(theme.faint).whitespace_normal().child(meta))
                .when(!review.is_empty(), |row| {
                    row.child(div().text_color(theme.warn).child(review.join("  ·  ")))
                }),
        )
}

fn monitor(recording: &Recording, inspected: Option<usize>, playhead: f64, theme: Theme, height: f32) -> gpui::Div {
    let cue = inspected.and_then(|index| recording.cues.get(index));
    let active = cue.filter(|cue| cue.start_ms as f64 <= playhead && playhead < cue.end_ms as f64);
    let upcoming = cue.filter(|_| active.is_none());
    let speaker = cue.and_then(|cue| cue.speaker.clone()).unwrap_or_default();
    let color = speaker_color(&speaker);
    let lines: Vec<String> = cue
        .map(|cue| cue.text.lines().map(str::to_string).collect())
        .unwrap_or_default();

    let tight = height < 190.0;
    let mut screen = div()
        .size_full()
        .relative()
        .overflow_hidden()
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
        .bottom(px(if tight { 12.0 } else { 28.0 }))
        .left(px(if tight { 12.0 } else { 48.0 }))
        .right(px(if tight { 12.0 } else { 48.0 }))
        .flex()
        .flex_col()
        .items_center()
        .gap(px(if tight { 3.0 } else { 6.0 }));
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
                .w_full()
                .px(px(if tight { 2.0 } else { 10.0 }))
                .py(px(2.))
                .rounded(px(3.))
                .bg(with_alpha(theme.monitor, 0.0))
                .child(
                    div()
                        .px(px(8.))
                        .py(px(1.))
                        .bg(gpui::rgba(0x000000B0))
                        .text_size(px(if tight { 15.0 } else { 22.0 }))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.bone)
                        .whitespace_normal()
                        .text_center()
                        .child(line),
                ),
        );
    }
    if upcoming.is_some() {
        stack = stack.opacity(0.45);
    }
    screen = screen.child(stack);

    div()
        .px(px(if tight { 12.0 } else { 24.0 }))
        .pb(px(4.))
        .child(
            div()
                .h(px(height))
                .w_full()
                .overflow_hidden()
                .rounded(px(12.))
                .bg(rgb_bezel())
                .border_1()
                .border_color(theme.line)
                .p(px(if tight { 4.0 } else { 8.0 }))
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
    slim: bool,
) -> gpui::Div {
    let duration = recording.duration_ms.max(1) as f64;
    let lane_of = srt::lanes(&recording.cues);
    let lane_count = lane_of.iter().copied().max().unwrap_or(0).min(3) + 1;
    let end = origin + span;
    let bars = waveform_bars(&recording.peaks, origin, span, duration, if slim { 64 } else { 120 });

    let mut wave = div().h(px(if slim { 22.0 } else { 36.0 })).w_full().flex().items_end().gap(px(1.));
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
    slim: bool,
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
                .child(mono_cell(if slim { 28.0 } else { 36.0 }, format!("{:02}", cue.index), theme.faint))
                .child(mono_cell(if slim { 96.0 } else { 118.0 }, srt::srt_stamp(cue.start_ms), theme.muted))
                .when(!slim, |row| row.child(mono_cell(118.0, srt::srt_stamp(cue.end_ms), theme.muted)))
                .child(mono_cell(40.0, cps, if note.fast || note.short { theme.warn } else { theme.muted }))
                .when(!slim, |row| {
                    row.child(
                        div()
                            .w(px(92.))
                            .flex_none()
                            .truncate()
                            .text_size(px(11.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(speaker_color(&name))
                            .child(if name.is_empty() { "—".to_string() } else { name }),
                    )
                })
                .child(div().flex_1().min_w_0().truncate().child(cue.plain())),
        );
    }

    div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .child(sheet_header(theme, slim))
        .child(rows)
}

fn sheet_header(theme: Theme, slim: bool) -> gpui::Div {
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
        .child(header_cell(if slim { 28.0 } else { 36.0 }, "#"))
        .child(header_cell(if slim { 96.0 } else { 118.0 }, "IN"))
        .when(!slim, |row| row.child(header_cell(118.0, "OUT")))
        .child(header_cell(40.0, "CPS"))
        .when(!slim, |row| row.child(header_cell(92.0, "SPEAKER")))
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

fn sidebar_footer(
    app: gpui::WeakEntity<Studio>,
    models: gpui::WeakEntity<Studio>,
    theme: Theme,
    toast: Option<String>,
    models_on: bool,
) -> gpui::Div {
    let mut footer = div()
        .p(px(12.))
        .border_t_1()
        .border_color(theme.line)
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(
            div()
                .id("open-models")
                .h(px(32.))
                .w_full()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .cursor_pointer()
                .text_size(px(12.))
                .font_weight(FontWeight::SEMIBOLD)
                .border_1()
                .border_color(if models_on { theme.bone } else { theme.line })
                .text_color(if models_on { theme.bone } else { theme.muted })
                .child("Models")
                .on_click(move |_event, _window, cx| {
                    models.update(cx, |this, cx| {
                        this.screen = Screen::Models;
                        cx.notify();
                    }).ok();
                }),
        )
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

fn text_button(
    id: &'static str,
    label: &'static str,
    app: gpui::WeakEntity<Studio>,
    theme: Theme,
    action: fn(&mut Studio, &mut Context<Studio>),
) -> Stateful<gpui::Div> {
    div()
        .id(id)
        .h(px(26.))
        .px(px(8.))
        .flex()
        .items_center()
        .rounded(px(6.))
        .cursor_pointer()
        .text_size(px(12.))
        .text_color(theme.muted)
        .border_1()
        .border_color(theme.line)
        .child(label)
        .on_click(move |_event, _window, cx| {
            app.update(cx, |this, cx| action(this, cx)).ok();
        })
}

fn model_card(
    theme: Theme,
    title: &str,
    engine: &str,
    field_id: &'static str,
    state: &Entity<EditableTextState>,
    placeholder: &'static str,
    status: &str,
) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(div().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).child(title.to_string()))
        .child(div().text_size(px(12.)).text_color(theme.faint).child(engine.to_string()))
        .child(model_field(theme, field_id, state, placeholder))
        .child(
            div()
                .font_family("JetBrains Mono")
                .text_size(px(11.))
                .text_color(theme.muted)
                .child(status.to_string()),
        )
}

fn model_field(
    theme: Theme,
    id: &'static str,
    state: &Entity<EditableTextState>,
    placeholder: &'static str,
) -> impl IntoElement {
    text_input(id)
        .state(state.downgrade())
        .placeholder(placeholder)
        .placeholder_color(theme.faint)
        .caret_color(gpui::rgb_to_hsla(theme.bone))
        .w_full()
        .min_h_auto()
        .max_h_auto()
        .whitespace_nowrap()
        .px(px(10.))
        .py(px(6.))
        .rounded(px(7.))
        .text_size(px(12.))
        .font_family("JetBrains Mono")
        .border_1()
        .border_color(theme.line)
        .bg(theme.bg)
}

fn package_page(
    recording: Option<&Recording>,
    selected: Option<&str>,
    app: gpui::WeakEntity<Studio>,
    theme: Theme,
) -> Stateful<gpui::Div> {
    let Some(recording) = recording else {
        return quiet(theme, "Open a recording to see its package.");
    };
    let bundle = &recording.bundle;
    if !bundle.packaged && bundle.concepts.is_empty() {
        return quiet(theme, "This session has no OKF package.");
    }
    let chosen = bundle
        .concepts
        .iter()
        .find(|concept| Some(concept.id.as_str()) == selected)
        .or_else(|| bundle.concepts.first());
    let mut page = div()
        .id("package-scroll")
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .px(px(16.))
        .pb(px(20.))
        .flex()
        .flex_col()
        .gap(px(12.))
        .child(section_label(theme, "PACKAGE"))
        .child(
            div()
                .text_size(px(16.))
                .font_weight(FontWeight::SEMIBOLD)
                .whitespace_normal()
                .child(if bundle.title.is_empty() { recording.title.clone() } else { bundle.title.clone() }),
        )
        .child(
            div()
                .text_size(px(12.))
                .text_color(theme.faint)
                .whitespace_normal()
                .child(format!(
                    "OKF {} · {} concepts · {} assets",
                    if bundle.version.is_empty() { "bundle" } else { bundle.version.as_str() },
                    bundle.concepts.len(),
                    bundle.assets.len()
                )),
        );
    if !bundle.description.is_empty() {
        page = page.child(
            div()
                .text_size(px(12.))
                .text_color(theme.muted)
                .whitespace_normal()
                .child(bundle.description.clone()),
        );
    }
    page = page.child(section_label(theme, "CONCEPTS"));
    for concept in &bundle.concepts {
        let on = chosen.map(|item| item.id == concept.id).unwrap_or(false);
        let id = concept.id.clone();
        let app = app.clone();
        page = page.child(
            div()
                .id(format!("concept-{id}"))
                .px(px(8.))
                .py(px(6.))
                .rounded(px(6.))
                .cursor_pointer()
                .bg(if on { theme.raised } else { theme.bg })
                .flex()
                .flex_col()
                .gap(px(2.))
                .on_click(move |_event, _window, cx| {
                    app.update(cx, |this, cx| {
                        this.concept = Some(id.clone());
                        this.pane = Pane::Package;
                        cx.notify();
                    })
                    .ok();
                })
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            div()
                                .text_size(px(10.))
                                .text_color(theme.faint)
                                .child(concept.type_name.clone()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_size(px(12.))
                                .font_weight(FontWeight::MEDIUM)
                                .child(concept.title.clone()),
                        ),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(if concept.status != "stable" { theme.warn } else { theme.muted })
                        .child(format!("{} · {}", concept.status, concept.trust.label())),
                ),
        );
    }
    if let Some(concept) = chosen {
        page = page.child(concept_detail(concept, theme));
    }
    page = page.child(section_label(theme, "ASSETS"));
    if bundle.assets.is_empty() {
        page = page.child(div().text_size(px(12.)).text_color(theme.faint).child("No assets in this package."));
    }
    for asset in &bundle.assets {
        page = page.child(
            div()
                .flex()
                .justify_between()
                .gap(px(8.))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .font_family("JetBrains Mono")
                        .text_size(px(12.))
                        .child(asset.name.clone()),
                )
                .child(
                    div()
                        .font_family("JetBrains Mono")
                        .text_size(px(11.))
                        .text_color(theme.faint)
                        .child(format_bytes(asset.bytes)),
                ),
        );
    }
    if !bundle.log.is_empty() {
        page = page.child(section_label(theme, "LOG")).child(
            div()
                .p(px(10.))
                .rounded(px(8.))
                .bg(theme.stage)
                .text_size(px(12.))
                .text_color(theme.muted)
                .whitespace_normal()
                .child(bundle.log.clone()),
        );
    }
    page
}

fn concept_detail(concept: &Concept, theme: Theme) -> gpui::Div {
    let mut detail = div()
        .p(px(10.))
        .rounded(px(8.))
        .bg(theme.stage)
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(section_label(theme, "CONCEPT"))
        .child(div().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).whitespace_normal().child(concept.title.clone()));
    if !concept.description.is_empty() {
        detail = detail.child(div().text_size(px(12.)).text_color(theme.muted).whitespace_normal().child(concept.description.clone()));
    }
    if !concept.resource.is_empty() {
        detail = detail.child(path_block(&concept.resource, theme));
    }
    if !concept.tags.is_empty() {
        detail = detail.child(
            div()
                .text_size(px(11.))
                .text_color(theme.faint)
                .whitespace_normal()
                .child(concept.tags.join(" · ")),
        );
    }
    if !concept.generated_by.is_empty() {
        detail = detail.child(div().text_size(px(11.)).text_color(theme.faint).whitespace_normal().child(format!("Generated by {}", concept.generated_by)));
    }
    if !concept.verified_by.is_empty() {
        detail = detail.child(
            div()
                .text_size(px(11.))
                .text_color(theme.faint)
                .whitespace_normal()
                .child(format!("Verified by {}", concept.verified_by.join(", "))),
        );
    }
    if !concept.body.is_empty() {
        detail = detail.child(div().text_size(px(12.)).whitespace_normal().child(concept.body.clone()));
    }
    detail
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.0} KB", bytes as f32 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f32 / (1024.0 * 1024.0))
    }
}
