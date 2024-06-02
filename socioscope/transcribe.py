audio_file_types = (".mp3", ".wav", ".flac", ".aac", ".m4a", ".ogg")

def transcribe_audio(file_path):

    if not file_path.lower().endswith(audio_file_types):
        raise ValueError("The file must be an audio file.")
    
    print(f"Transcribing {file_path}...")