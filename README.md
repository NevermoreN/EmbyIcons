## NevermoreN fork

This fork is displayed as **EmbyIcons (NevermoreN)** and uses plugin ID `5ba28f85-65d0-4c24-b4b2-4f195826c549`. Both its ID and display name differ from the upstream catalog entry, so Emby does not match it to official EmbyIcons updates. Configuration pages use the same fork ID. Other plugins keep their normal update behavior.

The source baseline is **5.61.1.5**, with informational version `5.61.1.5-nevermore.1`. This is not a copy of upstream 5.61.1.7.

Build with the .NET 8 SDK:

```sh
dotnet build EmbyIcons.csproj -c Release
```

Back up the existing plugin and `plugins/configurations/embyicons.xml`, stop Emby, then copy `bin/Release/netstandard2.0/EmbyIcons.dll` to `plugins/embyicons.dll` and restart Emby. Keep that exact lowercase destination filename so Emby reuses `embyicons.xml` and the existing `plugins/embyicons` data directory. Replace the upstream DLL; do not leave two copies loaded.

Install future versions by building this repository and replacing the same DLL while Emby is stopped. This fork does not automatically download GitHub builds. Do not manually install the official catalog entry alongside it.

---

![image](https://github.com/user-attachments/assets/08e70e95-d4fd-45be-b206-1d0b0922fbe1)

# EmbyIcons Plugin

[![Made for Emby](https://img.shields.io/badge/made%20for-emby-00a4dc.svg)](https://emby.media/)
[![BuyMeACoffee](https://img.shields.io/badge/support-buy%20me%20a%20coffee-yellow.svg)](https://buymeacoffee.com/yockser)

EmbyIcons enhances your Emby Server by overlaying informational icons directly onto media posters. Display language, codecs, resolution, ratings, and more at a glance with fully customizable profiles for different libraries.

---

## Features
*   **Extensive Icon Support**: Audio/subtitle languages, original language, audio channels, codecs (audio & video), HDR/Dolby Vision, resolution, frame rate, aspect ratio, parental ratings, community ratings (IMDb, Rotten Tomatoes, MDBList audience scores), custom tags, and favorite counts
*   **Multi-Profile System**: Create distinct profiles and assign them to different libraries for complete customization
*   **Flexible Layout Control**: Position icons in any corner with custom priority ordering and horizontal/vertical stacking
*   **Series & Collection Aggregation**: Smart detection shows icons only when all episodes/items share the same property (with Lite Mode option for faster scans)
*   **Profile Import/Export**: Backup and share your profile configurations
*   **Icon Manager**: Identify used, unused, and missing icons in your collection
*   **Series Troubleshooter**: Find episodes with inconsistent properties causing missing icons
*   **MDBList Integration**: Display Rotten Tomatoes audience scores with a free API key
*   **Live Preview**: See layout changes in real-time before saving
*   **Advanced Settings**: Fine-tune cache sizes, performance, and memory usage

---

## Installation
1.  Go to the catalog in Emby and find EmbyIcons under general.
2.  Press install.

---

## Icon Naming Convention
Place your icon images in a folder on your server. Recommended: ~100x100px PNG with transparency. Supported formats: `.png`, `.jpg`, `.webp`, `.gif`, `.bmp`.

Icons use the format: **`prefix.name.png`** (case-insensitive)

| Prefix | Type | Example |
|--------|------|---------|
| `lang` | Audio Language | `lang.english.png` |
| `sub` | Subtitle Language | `sub.german.png` |
| `og` | Original Language | `og.japanese.png` |
| `ch` | Audio Channels | `ch.7.1.png`, `ch.stereo.png` |
| `ac` | Audio Codec | `ac.dts.png`, `ac.eac3.png` |
| `vc` | Video Codec | `vc.hevc.png`, `vc.av1.png` |
| `hdr` | Video Format | `hdr.dv.png`, `hdr.hdr.png` |
| `res` | Resolution | `res.4k.png`, `res.1080p.png` |
| `fps` | Frame Rate | `fps.24.png`, `fps.60.png` |
| `ar` | Aspect Ratio | `ar.16x9.png`, `ar.2.39x1.png` |
| `pr` | Parental Rating | `pr.pg-13.png`, `pr.tv-ma.png` |
| `tag` | Custom Tag | `tag.3d.png`, `tag.directors-cut.png` |
| `rating` | Community Rating | `rating.imdb.png` |
| (none) | Rotten Tomatoes | `t.tomato.png`, `t.splat.png` |
| (none) | Popcorn-O-Meter | `t.popcorn.png`, `t.spilledpopcorn.png`, `t.fresh.png` |
| (none) | Favorite Count | `heart.png` |

**Note**: Rotten Tomatoes Popcorn-O-Meter requires a free [MDBList API key](https://mdblist.com) configured in Advanced Settings.

---

## Configuration Overview

### Settings Tab
*   **Global Settings**: Set icon folder path, loading mode (Hybrid recommended), output format, quality, and logging
*   **Profile Management**: Create, rename, delete, export, and import profiles
*   **Per-Profile Settings**:
    *   Assign profiles to libraries
    *   Enable/disable icon types with custom alignment (corner), priority, and layout (horizontal/vertical)
    *   Configure TV show/collection aggregation (Lite Mode, exclude Specials season, etc.)
    *   Customize rating score background (shape, color, opacity)
    *   Adjust icon size

### Icon Manager Tab
Scan your library to identify **missing**, **found**, and **unused** icons. Perfect for organizing your icon collection.

### Troubleshooter Tab
*   **Series Troubleshooter**: Find episodes with inconsistent properties (resolution, codec, etc.) that prevent series-level icons
*   **Aspect Ratio Calculator**: Get the exact icon filename for custom aspect ratios
*   **Memory Usage**: View plugin memory statistics

### Advanced Tab
Fine-tune cache sizes, expiration times, concurrency, and MDBList API key. Default values work for most users.

---

## Acknowledgements
Many thanks to **Neminem**, **keitaro26**, **Bakes82**, and everyone from the Emby Community forum. Shoutout to **Craggles** for the default icons.

If you enjoy this plugin, consider supporting development:

<a href="https://buymeacoffee.com/yockser" target="_blank"><img src="https://cdn.buymeacoffee.com/buttons/v2/default-yellow.png" alt="Buy Me A Coffee" style="height: 60px !important;width: 217px !important;" ></a>
