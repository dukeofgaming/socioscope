import argparse
import subprocess
import sys

def main():
    parser = argparse.ArgumentParser(
        prog        = __package__,
        description = "socioscope CLI tool"
    )

    subparsers = parser.add_subparsers(
        dest        = 'command', 
        required    = False
    )

    # transcribe_parser = subparsers.add_parser(
    #     'transcribe', 
    #     help='Transcribe an m4a audio file'
    # )

    # transcribe_parser.add_argument(
    #     'file', 
    #     type=str, 
    #     help='The path to the m4a file to transcribe', 
    #     metavar='FILE'
    # )

    args = parser.parse_args()

    if args.command == 'transcribe':
        # file_path = args.file
        # if not file_path.lower().endswith('.m4a'):
        #     print("Error: The file must be an m4a audio file.")
        #     sys.exit(1)
        print(f"Transcribing {args.file}...")
    else:
        parser.print_help()
        