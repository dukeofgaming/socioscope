from setuptools import setup, find_packages

setup(
    name='socioscope',
    version='0.1.0',
    packages=find_packages(),
    install_requires=[
        'pytest>=6.0.0',
        'pytest-mock>=3.6.1',
    ],
    entry_points={
        'console_scripts': [
            'socioscope=socioscope:main',
        ],
    },
)