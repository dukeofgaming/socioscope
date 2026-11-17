use std::path::{Path, PathBuf};

/// `os.path.splitext` on a file name. A leading dot is not an extension.
pub fn splitext(file_name: &str) -> (&str, &str) {
    let rest = file_name.trim_start_matches('.');
    let leading = file_name.len() - rest.len();
    match rest.rfind('.') {
        Some(index) => {
            let split_at = leading + index;
            (&file_name[..split_at], &file_name[split_at..])
        }
        None => (file_name, ""),
    }
}

/// Directory beside the source file. Matches `basename.split('.')[0]`, which
/// keeps only the text before the first dot.
pub fn get_output_directory_path(file_path: &Path) -> PathBuf {
    let name = file_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    let stem = name.split('.').next().unwrap_or("");
    match file_path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join(stem),
        _ => PathBuf::from(stem),
    }
}

/// Wav passed to whisper and diarization. An existing wav is used as-is.
pub fn get_source_wav_path(file_path: &Path) -> PathBuf {
    let name = file_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    let extension = splitext(name).1;
    if extension.ends_with(".wav") {
        file_path.to_path_buf()
    } else {
        get_output_directory_path(file_path).join("converted.wav")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitext_matches_python() {
        assert_eq!(splitext("jfk.wav"), ("jfk", ".wav"));
        assert_eq!(splitext("fdr.mp3"), ("fdr", ".mp3"));
        assert_eq!(splitext("audio.txt"), ("audio", ".txt"));
        assert_eq!(splitext("a.tar.gz"), ("a.tar", ".gz"));
        assert_eq!(splitext(".wav"), (".wav", ""));
    }

    #[test]
    fn output_paths_match_the_python_helpers() {
        let wav = Path::new("./tests/data/jfk.wav");
        assert_eq!(
            get_output_directory_path(wav),
            PathBuf::from("./tests/data/jfk")
        );
        assert_eq!(get_source_wav_path(wav), wav);

        let mp3 = Path::new("./tests/data/fdr.mp3");
        assert_eq!(
            get_output_directory_path(mp3),
            PathBuf::from("./tests/data/fdr")
        );
        assert_eq!(
            get_source_wav_path(mp3),
            PathBuf::from("./tests/data/fdr/converted.wav")
        );

        assert_eq!(
            get_output_directory_path(Path::new("audio.txt")),
            PathBuf::from("audio")
        );
    }
}
