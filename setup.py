from setuptools import setup, find_packages

setup(
    name='socioscope',
    version='0.1.0',
    packages=find_packages(),
    install_requires=[
        # List your project dependencies here.
        # For example: 'requests>=2.25.1',
    ],
    entry_points={
        'console_scripts': [
            'socioscope=socioscope:main',
        ],
    },
)