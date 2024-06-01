import subprocess

def main():
    subprocess.run(['echo', 'Hello World'], check=True)

if __name__ == "__main__":
    main()