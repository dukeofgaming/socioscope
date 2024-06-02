

def transcribe_audio(file_path):

    if not file_path.lower().endswith('.m4a'):
        raise ValueError("The file must be an m4a audio file.")
    
    print(f"Transcribing {file_path}...")