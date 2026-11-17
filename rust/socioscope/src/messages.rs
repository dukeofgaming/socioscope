pub const INVALID_AUDIO_TRANSCRIPTION_FORMAT: &str =
    "The file must be an audio file: {file_extensions}";

pub const SUPPORTED_AUDIO_FORMATS: &[&str] = &[".wav", ".m4a", ".mp3"];

pub fn invalid_audio_format() -> String {
    INVALID_AUDIO_TRANSCRIPTION_FORMAT
        .replace("{file_extensions}", &SUPPORTED_AUDIO_FORMATS.join(", "))
}
