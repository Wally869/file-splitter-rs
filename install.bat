@echo off
cargo build --release || exit /b 1
copy /Y target\release\file_splitter_rs.exe C:\Tools\
echo Installed file_splitter_rs to C:\Tools\
