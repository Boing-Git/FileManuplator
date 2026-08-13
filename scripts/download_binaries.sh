#!/usr/bin/env bash
set -e

# Downloads static portable versions of the required backends into bin/
mkdir -p bin

echo "Downloading FFmpeg..."
wget -qO- https://johnvansickle.com/ffmpeg/releases/ffmpeg-release-amd64-static.tar.xz | tar -xJ -C bin --strip-components=1 --wildcards '*/ffmpeg'

echo "Downloading Pandoc..."
wget -qO- https://github.com/jgm/pandoc/releases/download/3.1.9/pandoc-3.1.9-linux-amd64.tar.gz | tar -xz -C bin --strip-components=2 pandoc-3.1.9/bin/pandoc

echo "Downloading ImageMagick..."
wget -q https://github.com/ImageMagick/ImageMagick/releases/download/7.1.1-21/ImageMagick--gcc-x86_64.AppImage -O bin/magick
chmod +x bin/magick bin/ffmpeg bin/pandoc

echo "Successfully downloaded portable binaries into bin/!"
