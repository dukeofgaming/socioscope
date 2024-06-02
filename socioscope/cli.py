import argparse
import sys

from socioscope import transcribe

def main():
    parser = argparse.ArgumentParser(
        prog        = __package__,
        description = "socioscope CLI tool"
    )

    subparsers = parser.add_subparsers(
        dest        = 'command', 
        required    = False
    )

    transcribe_parser = subparsers.add_parser(
        'transcribe', 
        help='Transcribe an m4a audio file'
    )

    transcribe_parser.add_argument(
        'file', 
        type=str, 
        help='The path to the m4a file to transcribe', 
        metavar='FILE'
    )

    args = parser.parse_args()

    if args.command == 'transcribe':

        try:
            transcribe.transcribe_audio(args.file)
        except ValueError as exception:
            print(exception, file=sys.stderr)

    else:
        parser.print_help()
        