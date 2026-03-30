@echo off
echo Building VRCMediaController...
pip install -r requirements.txt
pip install pyinstaller

echo Running PyInstaller...
pyinstaller --noconfirm --onedir --windowed --add-data "config.json;." main.py

echo Build complete! Check the dist/main directory.
pause
