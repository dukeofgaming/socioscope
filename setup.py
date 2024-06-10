from setuptools import setup, find_packages

setup(
    name        = 'socioscope',
    version     = '0.1.0',
    packages    = find_packages(),

    install_requires    = [
        # Core
        'pytest>=6.0.0',
        'pytest-mock>=3.6.1',

        # Configuration
        'pyyaml>=6.0.1',

        # Diarization
        'pyannote.audio>=3.2.0',
        'torchvision>=0.18.1',
    ],

    entry_points        = {
        'console_scripts': [
            'socioscope=socioscope.cli:main',
        ],
    },
)