#!/bin/bash

# Check if the script is running with sudo
if [ "$EUID" -ne 0 ]; then
  echo "Please run this script with sudo."
  exit 1
fi

# Fetch the latest release information from GitHub API
release_info=$(curl --silent "https://api.github.com/repos/jaerod95/pr/releases/latest")

# Parse the download URL of the release asset
download_url=$(echo "$release_info" | grep -Eo 'browser_download_url.*$')
download_url=${download_url:23}

# Extract the filename from the download URL
filename=$(basename "$download_url")

# Download the release binary
echo "Downloading $filename..."
curl -LJO "$download_url"

# Make the downloaded binary executable
chmod +x "$filename"

# Move the binary to /usr/bin
echo "Moving $filename to /usr/bin..."
mv "$filename" /usr/bin/pr

echo "Installation completed!"
