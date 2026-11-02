#!/usr/bin/env bash
set -e

cargo build --release
cp target/release/file_splitter_rs.exe C:/Tools/
echo "Installed file_splitter_rs to C:/Tools/"
