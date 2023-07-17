#!/bin/bash

# Requires semver to be installed `npm install -g semver``

# Check if the current branch is "main"
branch=$(git rev-parse --abbrev-ref HEAD)
if [ "$branch" != "main" ]; then
  echo "This script can only be run on the 'main' branch. Aborting."
  exit 1
fi

echo "Which version bump would you like to apply?"
echo "1. Patch (0.0.1 -> 0.0.2)"
echo "2. Minor (0.1.0 -> 0.2.0)"
echo "3. Major (1.0.0 -> 2.0.0)"
read -p "Enter your choice [1-3]: " choice

case $choice in
    1)
        bump="patch"
        ;;
    2)
        bump="minor"
        ;;
    3)
        bump="major"
        ;;
    *)
        echo "Invalid choice. Exiting."
        exit 1
        ;;
esac

current_version=$(sed -nE 's/^version = "(.*)"/\1/p' Cargo.toml)

new_version=$(semver -i $bump $current_version)
awk -v new_version="$new_version" '/^version = / {$0 = "version = \"" new_version "\""} {print}' Cargo.toml > Cargo.toml.tmp && mv Cargo.toml.tmp Cargo.toml

echo "Updated version: $new_version"

echo "Sleeping so the Cargo.lock can change..."
sleep 3

git add . && git commit -m "Bump version to $new_version" && git push origin main

cargo build --release
release_binary="target/release/pr_opener"

echo "Creating GitHub release..."
gh release create "v$new_version" "$release_binary" --notes "Release $new_version" --generate-notes

echo "Release created and binary uploaded."