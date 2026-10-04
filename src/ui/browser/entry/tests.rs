// SPDX-License-Identifier: MIT

use super::icon_for_name;
use crate::assets::icons;

#[test]
fn audio_extensions_resolve_to_the_audio_icon() {
    for name in [
        "song.mp3",
        "take.WAV",
        "master.flac",
        "clip.ogg",
        "album.m4a",
        "track.aac",
        "voice.opus",
        "legacy.wma",
        "sample.aiff",
    ] {
        assert_eq!(
            icon_for_name(name),
            icons::FILE_AUDIO,
            "audio file shows the audio icon: {name}"
        );
    }
}

#[test]
fn categorized_extensions_resolve_to_their_category_icons() {
    let cases = [
        ("page.html", icons::GLOBE),
        ("page.htm", icons::GLOBE),
        ("style.css", icons::GLOBE),
        ("style.scss", icons::GLOBE),
        ("feed.xml", icons::GLOBE),
        ("backup.zip", icons::FILE_ARCHIVE),
        ("backup.7z", icons::FILE_ARCHIVE),
        ("backup.tar", icons::FILE_ARCHIVE),
        ("backup.tar.gz", icons::FILE_ARCHIVE),
        ("backup.tgz", icons::FILE_ARCHIVE),
        ("backup.tar.xz", icons::FILE_ARCHIVE),
        ("backup.bz2", icons::FILE_ARCHIVE),
        ("backup.xz", icons::FILE_ARCHIVE),
        ("backup.zst", icons::FILE_ARCHIVE),
        ("backup.rar", icons::FILE_ARCHIVE),
        ("installer.deb", icons::BOX),
        ("installer.rpm", icons::BOX),
        ("installer.pkg", icons::BOX),
        ("installer.AppImage", icons::BOX),
        ("installer.msi", icons::BOX),
        ("installer.exe", icons::BOX),
        ("server.pem", icons::KEY_ROUND),
        ("server.crt", icons::KEY_ROUND),
        ("server.cer", icons::KEY_ROUND),
        ("server.key", icons::KEY_ROUND),
        ("server.der", icons::KEY_ROUND),
        ("server.csr", icons::KEY_ROUND),
        ("config.yaml", icons::COG),
        ("config.yml", icons::COG),
        ("config.toml", icons::COG),
        ("config.ini", icons::COG),
        ("config.conf", icons::COG),
        (".env", icons::COG),
        ("data.json", icons::FILE_BRACES),
        ("data.jsonc", icons::FILE_BRACES),
        ("data.db", icons::DATABASE),
        ("data.sqlite", icons::DATABASE),
        ("data.sqlite3", icons::DATABASE),
        ("disk.iso", icons::DISC),
        ("disk.img", icons::DISC),
        ("disk.dmg", icons::DISC),
        ("disk.vhd", icons::DISC),
        ("disk.vhdx", icons::DISC),
        ("disk.vdi", icons::DISC),
        ("disk.qcow", icons::DISC),
        ("data.csv", icons::FILE_SPREADSHEET),
        ("sheet.xls", icons::EXCEL),
        ("sheet.xlsx", icons::EXCEL),
        ("sheet.ods", icons::EXCEL),
        ("main.rs", icons::FILE_CODE),
        ("main.py", icons::FILE_CODE),
        ("main.js", icons::FILE_CODE),
        ("main.ts", icons::FILE_CODE),
        ("main.c", icons::FILE_CODE),
        ("main.cpp", icons::FILE_CODE),
        ("main.h", icons::FILE_CODE),
        ("main.hpp", icons::FILE_CODE),
        ("Main.java", icons::FILE_CODE),
        ("Main.kt", icons::FILE_CODE),
        ("main.swift", icons::FILE_CODE),
        ("main.dart", icons::FILE_CODE),
        ("main.scala", icons::FILE_CODE),
        ("main.hs", icons::FILE_CODE),
        ("main.lua", icons::FILE_CODE),
        ("main.rb", icons::FILE_CODE),
        ("main.php", icons::FILE_CODE),
        ("run.sh", icons::FILE_TERMINAL),
        ("run.bash", icons::FILE_TERMINAL),
        ("run.zsh", icons::FILE_TERMINAL),
        ("run.fish", icons::FILE_TERMINAL),
        ("run.ksh", icons::FILE_TERMINAL),
        ("run.csh", icons::FILE_TERMINAL),
        ("run.ps1", icons::FILE_TERMINAL),
        ("run.bat", icons::FILE_TERMINAL),
        ("run.cmd", icons::FILE_TERMINAL),
        ("report.doc", icons::WORD),
        ("report.DOCX", icons::WORD),
        ("report.odt", icons::WORD),
        ("report.rtf", icons::WORD),
        ("deck.ppt", icons::POWERPOINT),
        ("deck.pptx", icons::POWERPOINT),
        ("deck.pps", icons::POWERPOINT),
        ("deck.ppsx", icons::POWERPOINT),
        ("deck.odp", icons::POWERPOINT),
        ("README.md", icons::FILE_MARKDOWN),
        ("notes.markdown", icons::FILE_MARKDOWN),
        ("notes.mdown", icons::FILE_MARKDOWN),
        ("notes.mkd", icons::FILE_MARKDOWN),
        ("font.ttf", icons::FILE_FONT),
        ("font.otf", icons::FILE_FONT),
        ("font.woff", icons::FILE_FONT),
        ("font.woff2", icons::FILE_FONT),
    ];
    for (name, icon) in cases {
        assert_eq!(icon_for_name(name), icon, "categorized file icon: {name}");
    }
}

#[test]
fn previously_mapped_types_keep_their_icons() {
    assert_eq!(icon_for_name("movie.mp4"), icons::VIDEOS);
    assert_eq!(icon_for_name("photo.png"), icons::PICTURES);
}

#[test]
fn unrecognized_files_fall_back_to_the_unknown_icon() {
    assert_eq!(icon_for_name("notes.txt"), icons::FILE);
    assert_eq!(icon_for_name("Makefile"), icons::FILE);
    assert_eq!(icon_for_name("archive.???"), icons::FILE);
}
