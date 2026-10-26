<p align="center"><img src="ui/public/westplay.png" width="112" alt="WestPlay"></p>

<h1 align="center">WestPlay</h1>

<p align="center">
  <b>A CarPlay-style screen for the trucks of Euro Truck Simulator 2.</b><br>
  Your map, your music and YouTube, on your truck's own dashboard screen.
</p>

<p align="center">
  <a href="https://github.com/enes-less/WestPlay/releases/latest"><img alt="Download for Windows" src="https://img.shields.io/badge/download-for%20Windows-409c42?style=for-the-badge"></a>
  <img alt="Windows 10 and 11" src="https://img.shields.io/badge/Windows-10%20%7C%2011-2b2b2b?style=for-the-badge">
  <img alt="Euro Truck Simulator 2 1.61" src="https://img.shields.io/badge/ETS2-1.61-2b2b2b?style=for-the-badge">
  <a href="LICENSE"><img alt="Licence: GPL-3.0-or-later" src="https://img.shields.io/badge/licence-GPL--3.0-2b2b2b?style=for-the-badge"></a>
</p>

<p align="center">
  <a href="#features">Features</a> ·
  <a href="#trucks">Trucks</a> ·
  <a href="#install">Install</a> ·
  <a href="#privacy">Privacy</a> ·
  <a href="#support">Support</a>
</p>

<br>

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/images/home-dark.png">
  <source media="(prefers-color-scheme: light)" srcset="docs/images/home-light.png">
  <img alt="WestPlay's Home screen: Maps, Music, Spotify, YouTube and YouTube Music on the truck's screen" src="docs/images/home-dark.png" width="100%">
</picture>

WestPlay puts a phone-style car screen on your truck's own dashboard display, inside the game. Your route on a
map made from your own game, your music, YouTube, the weather on your drive: right where a real truck has its
screen, used with the mouse while you hold a key.

## Features

<table>
  <tr>
    <td width="50%" valign="top">
      <picture>
        <source media="(prefers-color-scheme: dark)" srcset="docs/images/maps-dark.png">
        <source media="(prefers-color-scheme: light)" srcset="docs/images/maps-light.png">
        <img alt="Maps, around Frankfurt am Main" src="docs/images/maps-dark.png" width="100%">
      </picture>
      <h3>Maps from your own game</h3>
      Drawn from your game's map, map mods included (ProMods and others), with your route, turn-by-turn
      guidance, and search that adds stops to the game's own GPS.
    </td>
    <td width="50%" valign="top">
      <picture>
        <source media="(prefers-color-scheme: dark)" srcset="docs/images/dashboard-dark.png">
        <source media="(prefers-color-scheme: light)" srcset="docs/images/dashboard-light.png">
        <img alt="The Dashboard: the map beside what's playing" src="docs/images/dashboard-dark.png" width="100%">
      </picture>
      <h3>A Dashboard for the drive</h3>
      The map beside what's playing and where you're going, the weather on your drive, all on one screen.
    </td>
  </tr>
  <tr>
    <td width="50%" valign="top">
      <img alt="YouTube Music's Now Playing" src="docs/images/youtube-music.png" width="100%">
      <h3>Your music</h3>
      Apple Music, Spotify and YouTube Music, with Now Playing and lyrics. A song's music video plays right in
      Now Playing, and YouTube has its channels, playlists, captions and quality.
    </td>
    <td width="50%" valign="top">
      <picture>
        <source media="(prefers-color-scheme: dark)" srcset="docs/images/looks-light.png">
        <source media="(prefers-color-scheme: light)" srcset="docs/images/looks-dark.png">
        <img alt="Home in its other look" src="docs/images/looks-light.png" width="100%">
      </picture>
      <h3>Light and dark, and your own look</h3>
      It turns dark at night or when you switch your headlights on, like a car's light sensor. Bring your own
      wallpapers and app icons.
    </td>
  </tr>
</table>

**The companion app** on your PC sets everything up: it finds the game, installs WestPlay into it, builds your
map, and keeps WestPlay up to date.

## How it works

```mermaid
flowchart LR
  game["Euro Truck Simulator 2"] -- "the truck, its route" --> plugin["WestPlay's plugin<br/>(in the game)"]
  plugin <--> app["WestPlay<br/>(on your PC)"]
  app -- "the screen, drawn in WestPlay's own browser" --> plugin
  plugin -- "draws it on" --> screen["The truck's own screen"]
```

WestPlay draws its screen in a browser of its own on your PC, and its plugin puts each frame on your truck's
display, inside the game. The trucks' own screens and their information bars stay theirs: WestPlay takes the
GPS's place, and its Vehicle Bar setting says whether the truck's bar stays beside it.

## Trucks

WestPlay shows up in two ways: on the truck's own screen, or on the WestPlay tablet.

**On the truck's own screen.** In the trucks that have CarPlay in real life, WestPlay takes the GPS's place on
the dashboard screen, shaped to it. There's nothing to buy or fit: get in and it's there.

- **DAF** XF and XG/XG+ 2021, XF Electric and XD (and Jasper's DAF 2021)
- **Scania** S and R, with the 7" screen or the Smart Dash, and the S BEV
- **Iveco** S-Way
- **Renault** T (2024)
- **Volvo** FH (2021 and 2024)
- **Oyuncuyusbis' coaches:** the Mercedes-Benz Travego 16 SHD, the MAN Lion's Coach and the Setra S 531 DT on their
  dashboard screens
- **Setra S 516 HD/2** (Oyuncuyusbis'): on the coach's own GPS screen, its "Gps" part, which you fit in the truck
  shop: the coach's own screen, as its players asked, but one you add

**On the WestPlay tablet.** Every other truck, the game's and mod trucks alike, gets WestPlay on a tablet you fit
in the truck shop: under Accessories, in Hanging Items or as a dashboard set. The truck's own screen keeps the
game's navigation. The tablet is in every truck with hanging items, including truck mods you install later (they
get it the next time you start the game).

|  | Truck's own screen | WestPlay tablet |
|---|---|---|
| Trucks | The ones above | Every other truck |
| To get it | Nothing: it's there | Fit it in the truck shop |
| The game's navigation | Replaced by WestPlay | Stays on the truck's screen |

### Sound

Music plays from the truck's own speakers, each where it sits in the cab. Other trucks play plain stereo.

- **Scania S and R (2016), S BEV** — door woofers, A-pillar tweeters, roof bass
- **Scania R (2009), Streamline** — door woofers, A-pillar tweeters
- **DAF XF (105)** — A-pillar tweeters; two pairs on the overhead shelf in the Super Space Cab, one overhead unit in the others
- **DAF XF Euro 6** — door woofers, A-pillar tweeters; two pairs on the overhead shelf in the Super Space Cab, one overhead unit in the others
- **DAF XF, XG, XG+ (2021), XF Electric and XD** — door woofers, A-pillar tweeters, an overhead pair, a centre
- **MAN TGX** — door woofers; in the XXL, two small units high in each side wall too
- **MAN TGX Euro 6** — door woofers; in the XL, a small unit in each roof corner too
- **MAN TGX (2020)** — a woofer and a midrange in each door, A-pillar tweeters
- **Mercedes-Benz Actros (2014)** — door woofers, an overhead pair, A-pillar tweeters
- **Mercedes-Benz Actros (2009)** — door woofers, a pair at the overhead shelf's front corners
- **Volvo FH (2021, 2024), FH16 (2012)** — door woofers, back-wall bass
- **Volvo FH16 (2009)** — a unit in each door
- **Iveco Stralis** — a unit in each end of the overhead console, and in the high roofs a small one on the passenger's side
- **Iveco Hi-Way** — a unit in each end of the overhead console, and one high on each side
- **Iveco S-Way** — a woofer and a tweeter in each door
- **Renault Magnum** — a unit in each roof corner, tweeters on the overhead console
- **Renault Premium** — a unit in each side of the roof, lower in the low roof
- **Renault T** — A-pillar tweeters, a back-wall pair; door woofers in the 2024 interiors
- **Renault E-Tech T** — door woofers, A-pillar tweeters, a back-wall pair
- **JuseeTV's Scania PGR** — the speakers of the parts you fit: doors, A-pillars, top console, rear wall

## Install

1. Download the installer from the [latest release](https://github.com/enes-less/WestPlay/releases/latest) and run it.
2. The companion opens and walks you through the rest: it finds your game, installs WestPlay's plugin and its
   screens mod, and builds your map.
3. Start the game and get in a truck.

WestPlay looks for updates on its own and asks before installing one.

**Requirements:** Windows 10 or 11 (64-bit), Euro Truck Simulator 2 version 1.61 (other versions may work; the
companion says when one is untested), and Microsoft Edge WebView2 (part of Windows 11; the installer adds it
where it's missing).

## Privacy

WestPlay has no servers, no accounts of its own and no analytics. It doesn't collect or send anything about you.

- You sign in to Apple Music, Spotify and YouTube on their own pages, in WestPlay's own browser profile on your PC,
  as you would in a browser. WestPlay keeps no account data of its own.
- Your map is built from your game on your PC and stays there.
- WestPlay talks to the services you use, to GitHub to look for updates, and to Kreosus only if you open
  "Support me".

## Support

WestPlay is free. If it makes your drives better, you can support its development:

<p>
  <a href="https://kreosus.com/enes-less"><img alt="Support WestPlay on Kreosus" src="https://img.shields.io/badge/support%20on-Kreosus-409c42?style=for-the-badge"></a>
</p>

<details>
<summary><b>Building from source</b></summary>
<br>

You need [Rust](https://rustup.rs) (stable) and [Node.js](https://nodejs.org) 24, on Windows.

```sh
npm install      # the companion's tools and the UI's packages
npm run dev      # WestPlay, with the UI served live
```

`cargo test --workspace` and `npm test --prefix ui` run the tests. The game plugin is built with
`cargo build --release -p westplay-plugin`.

</details>

## Licence

WestPlay is free software: you can redistribute it and modify it under the terms of the GNU General Public
License, version 3 or (at your option) any later version. See [LICENSE](LICENSE).

The libraries WestPlay ships keep their own licences: see [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md), and
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) for the work it follows.

<sub>WestPlay is an independent project, not affiliated with or endorsed by Apple, SCS Software, Spotify, Google or
the truck makers. CarPlay, Apple Music, Spotify, YouTube, YouTube Music and the truck makers' names and logos are
trademarks of their owners, used to name what WestPlay works with. The screenshots show WestPlay's preview with
made-up songs.</sub>
