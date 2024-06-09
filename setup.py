from setuptools import setup, find_packages

setup(
    name        = 'socioscope',
    version     = '0.1.0',
    packages    = find_packages(),
    
    install_requires    = [
        # Testing
        'pytest>=6.0.0',
        'pytest-mock>=3.6.1',

        # Diarization
        'pyAudioAnalysis>=0.3.14',
        'scipy>=1.13.1',
        'scikit-learn>=1.5.0',
        'hmmlearn>+0.3.2'
        'pandas>=2.2.2',
        'matplotlib>=3.9.0',
        'eyed3>=0.9.7',
        'pydub>=0.25.1',
        'imbalanced-learn>=0.12.3',
        'plotly>=5.22.0'
    ],

    entry_points    = {
        'console_scripts': [
            'socioscope=socioscope.cli:main',
        ],
    },
)