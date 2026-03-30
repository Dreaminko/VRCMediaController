@echo off
echo Building VRCMediaController...
pip install -r requirements.txt
pip install pyinstaller

echo Running PyInstaller...
pyinstaller --noconfirm --onefile --windowed --add-data "config.json;." --add-data "fav.ico;." --icon="fav.ico" main.py

echo Build complete! Check the dist/main directory.
pause
