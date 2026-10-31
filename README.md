# AutoLyrics

**English** · [Русский](README.ru.md)

A soft, colourful Windows app that picks up whatever music is playing on your PC, shows **synced
lyrics** and draws an **audio visualizer matched to the genre**. The interface stays hidden —
everything (menu and settings) opens with a right click.

<p align="center">
  <img src="docs/screenshots/hero.jpg" alt="AutoLyrics: synced lyrics, album-art colours and a pop-style visualizer" width="900">
</p>

> The screenshots are taken in the app's built-in demo mode with sample data (made-up lyrics and
> artwork), so no real song texts are shown.

UI languages: English, Русский, Deutsch, Français, Polski, 中文, 日本語.

## Features

- **Finds the track by itself.** Uses the player's own data (Yandex Music, Spotify, Chrome/Edge, VLC…) or,
  if the player does not report it, **listens to the system audio** and identifies the song by an audio
  fingerprint.
- **Synced lyrics** from several sources, saved locally as plain `.lrc` files — you can drop in your own.
- **Genre-aware visualizer** with seven styles that crossfade smoothly; colours come from the album art.
- **Soft, animated interface:** search → found track → lyrics intro animations, optional
  *Liquid Glass* buttons, tray icon, autostart.
- **Seven languages**, picked on first run by detecting the language of your favourite song.
- **Zen mode:** just the visualizer, no lyrics — optionally with the album art, title and artist on top.
- **Your own colours** for the background and the equalizer, and a menu split into *Track / Look / App* tabs.

<table>
  <tr>
    <td><img src="docs/screenshots/visualizer-electronic.jpg" alt="Electronic style"></td>
    <td><img src="docs/screenshots/visualizer-hiphop.jpg" alt="Hip-hop style"></td>
    <td><img src="docs/screenshots/visualizer-rock.jpg" alt="Rock style"></td>
  </tr>
  <tr>
    <td align="center">Electronic</td>
    <td align="center">Hip-hop</td>
    <td align="center">Rock</td>
  </tr>
</table>

Also: pop (bubbles), chill (aurora), classical/jazz (thin flowing lines) and soft blobs.

## Install

1. Download `AutoLyrics_<version>_x64-setup.exe` from [Releases](../../releases).
2. Run it. It installs for the current user — no administrator rights needed — and you can pick the
   installer language.
3. **The installer is not code-signed**, so Windows SmartScreen may show "Unknown publisher":
   click *More info* → *Run anyway*. You can verify the file with the SHA-256 in the release notes.

Requirements: Windows 10 (1809+) or Windows 11 and WebView2 (already part of Windows 11). Windows only.

## First run

<table>
  <tr>
    <td><img src="docs/screenshots/onboarding.jpg" alt="First run: play your favourite song"></td>
    <td><img src="docs/screenshots/onboarding-detected.jpg" alt="Language detected from the lyrics"></td>
    <td><img src="docs/screenshots/language-picker.jpg" alt="Manual language picker"></td>
  </tr>
</table>

The greeting cycles through all seven languages and asks you to play your favourite song in your
native language. The app finds the song, looks up its lyrics and **detects the language from the text**.
There is also a button at the bottom to choose the language manually. You can change it any time in
the menu.

## How it works

- **Track.** If the player reports the track to Windows, we use that. Otherwise the app listens to the
  audio playing on your PC and identifies the song by a fingerprint (Shazam's algorithm); the match offset
  keeps the lyrics in sync. This also works for web players that do not report anything to the system.
- **Lyrics.** LRCLIB, NetEase, QQ Music and KuGou (line-synced) and lyrics.ovh (plain text) are queried in
  parallel; the result is picked by priority. What is found is saved to the `lyrics` folder next to the
  app as plain `.lrc` files. Put your own `Artist - Title.lrc` there: it always wins and is never
  overwritten. Optional: an official Musixmatch API key (the free tier returns partial, unsynced text).
- **Visualizer.** Spectrum, beats and tempo are computed from the audio. The style follows the genre
  (from Shazam or iTunes); if the genre is unknown it is estimated from the sound itself. While a track is
  being searched for, the visualizer is smoothed and blurred like the background.
- **End of track.** When a song ends, the app starts identifying the next one right away (important for
  gapless transitions). The periodic re-check of the current track can be switched off ("Auto-check track").
  Optional "guess the next album track" (off by default — it may briefly show the wrong
  song).

## Controls

| Action | How |
|---|---|
| Menu and all settings | right mouse button |
| Move the window | left mouse button |
| Fullscreen | double click |
| Hide to tray / bring back | "Minimize to tray" / click the tray icon |
| Quit | "Close" in the menu or "Quit" in the tray menu |

<p align="center">
  <img src="docs/screenshots/menu.jpg" alt="The right-click menu with Liquid Glass buttons" width="520">
</p>

## Troubleshooting

- **No lyrics found.** Rare songs may be missing from every source. Put your own `.lrc` into the
  `lyrics` folder (the "Lyrics folder" button in the menu) — it is picked up immediately.
- **Song is not identified.** It needs sound on the PC and an internet connection. Music from DRM-protected
  sources may be captured as silence. "Identify now" retries.
- **Antivirus or SmartScreen complains.** The app is unsigned and captures the system audio (WASAPI
  loopback) — expected for this kind of program; the source code is open.

## Privacy

No telemetry, no accounts. Settings are stored only on your computer. What leaves your PC:

| What | Where to |
|---|---|
| Title and artist (lyrics, genre and album lookup) | LRCLIB, NetEase, QQ Music, KuGou, lyrics.ovh, iTunes |
| An audio fingerprint — a few seconds of numeric data, **not the recording** | Shazam |
| Cover art, from links in the services' replies | cover servers |
| Title and artist + your key (only if you set one) | Musixmatch (official API) |

The audio itself is never saved or sent anywhere.

## Rights and third-party services

- **Song lyrics belong to their rights holders.** The app contains no lyrics and does not redistribute
  them: it fetches the text from third-party services and shows it locally for personal use. The saved
  `.lrc` files live on your disk — please do not publish them.
- **Third-party services are used without agreements with them.** Some of them (Shazam, NetEase, QQ Music,
  KuGou) are called through unofficial interfaces: they can change, rate-limit or stop working, and their
  terms of use apply regardless of this code's licence. Use at your own risk. Musixmatch is used only through
  its official API with your personal key.
- **Not affiliated with or endorsed by** Shazam/Apple, Yandex, Spotify, Musixmatch, Valve or any other
  company mentioned; all names belong to their owners.

## Support the project

The app is free and will stay free. After a day of use a "♥ Support the project" button appears in the
menu with a Steam trade link. **A donation is voluntary and gives nothing in return.** You can hide the
button for good ("Don't show again").

## License

[GPL-3.0-or-later](LICENSE). The app uses the [songrec](https://github.com/marin-m/SongRec) library
(GPL-3.0+) to compute the audio fingerprint, so it is distributed under the GPL as well: anyone who
receives the installer may get the source code (it is here), modify it and redistribute it under the
same terms. The other dependencies use permissive licences (MIT, Apache-2.0 and similar).

**Credits:** [SongRec](https://github.com/marin-m/SongRec) (audio fingerprinting),
[LRCLIB](https://lrclib.net) (open synced-lyrics database), [Tauri](https://tauri.app),
[cpal](https://github.com/RustAudio/cpal), [RustFFT](https://github.com/ejmahler/RustFFT),
[reqwest](https://github.com/seanmonstar/reqwest), [tokio](https://tokio.rs).

## Build from source

You need Rust (MSVC toolchain, Visual Studio Build Tools) and WebView2. Node.js is not needed — the
interface is plain JavaScript with no bundler.

```powershell
cd src-tauri
cargo run                 # run in development mode
cargo test                # tests
cargo install tauri-cli --version "^2" --locked
cargo tauri build --bundles nsis   # release build and installer
```

Debugging: set `AUTOLYRICS_DEBUG=1` to print what recognition and lyrics search are doing.
The interface also runs in a regular browser (demo mode): serve the `src` folder with any static server
and use the parameters `?viz=rock`, `?lang=de`, `?ob=1`, `?lyrics=de`, `?menu=1`, `?picker=1`, `?donate=1`.

## Project layout

```
src/                 interface (HTML/CSS/JS): background, visualizer, title card, menu, i18n
src-tauri/src/       Rust: media_watcher (SMTC), audio_capture (WASAPI loopback), analyzer (FFT),
                     recognizer (Shazam), lyrics/ (sources, .lrc folder), genre, album, predict, shell
docs/screenshots/    README screenshots (taken in demo mode)
```
